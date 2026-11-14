//! 工具行为：临时目录树里有软链接、FIFO、`..`、大文件、二进制文件和中文文件名。
//! 断言结构化结果，不依赖提示措辞。

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::Duration;

use filetime::{FileTime, set_file_mtime};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Resource};
use serde_json::Value;
use tempfile::TempDir;
use wonton_files::SHARES;
use wonton_files::tools::{
    KindParam, Limits, ListDirectory, MatchMode, ReadTextFile, SearchContent, SearchFiles,
};

fn setup() -> (TempDir, wonton_files::FilesServer, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    for share in SHARES {
        fs::create_dir(dir.path().join(share)).unwrap();
    }
    let server = wonton_files::FilesServer::new(dir.path());
    let root = dir.path().to_owned();
    (dir, server, root)
}

fn write(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    path
}

fn links(result: &CallToolResult) -> Vec<&Resource> {
    result
        .content
        .iter()
        .filter_map(|block| block.as_resource_link())
        .collect()
}

fn structured(result: &CallToolResult) -> &Value {
    result
        .structured_content
        .as_ref()
        .expect("工具结果里应有 structuredContent")
}

fn has_text(result: &CallToolResult, needle: &str) -> bool {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .any(|text| text.text.contains(needle))
}

fn assert_error(result: &CallToolResult, kind: &str) {
    assert_eq!(result.is_error, Some(true));
    assert_eq!(structured(result)["error"], kind);
}

fn search(query: &str) -> SearchFiles {
    SearchFiles {
        query: query.to_owned(),
        share: None,
        subdir: None,
        r#match: MatchMode::Substring,
        case_sensitive: false,
        modified_after: None,
        kind: None,
        offset: None,
        limit: None,
    }
}

fn read(path: &str) -> ReadTextFile {
    ReadTextFile {
        share: "documents".to_owned(),
        path: path.to_owned(),
        offset: None,
        limit: None,
    }
}

fn content(query: &str, max_size: Option<u64>) -> SearchContent {
    SearchContent {
        query: query.to_owned(),
        share: Some("documents".to_owned()),
        subdir: None,
        modified_after: None,
        kind: None,
        max_size,
        offset: None,
        limit: None,
    }
}

#[tokio::test]
async fn search_returns_encoded_resource_link() {
    let (_dir, server, root) = setup();
    write(&root, "documents/中文 文件.txt", "你好".as_bytes());
    let result = server
        .search_files(Parameters(search("中文")))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(false));
    let links = links(&result);
    assert_eq!(links.len(), 1);
    assert_eq!(
        links[0].uri,
        "wonton-share:documents/%E4%B8%AD%E6%96%87%20%E6%96%87%E4%BB%B6.txt"
    );
    assert_eq!(links[0].name, "中文 文件.txt");
    assert_eq!(links[0].mime_type.as_deref(), Some("text/plain"));
    assert_eq!(links[0].size, Some(6));
    assert_eq!(links[0].title.as_deref(), Some("documents/中文 文件.txt"));
    assert_eq!(structured(&result)["total"], 1);
    assert!(structured(&result)["next_offset"].is_null());
    assert!(has_text(&result, "wonton__share_file"));
}

#[tokio::test]
async fn glob_kind_and_modified_since() {
    let (_dir, server, root) = setup();
    write(&root, "photos/2025/a.jpg", b"jpg");
    let old = write(&root, "photos/2025/b.png", b"png");
    set_file_mtime(&old, FileTime::from_unix_time(1_600_000_000, 0)).unwrap();

    let mut request = search("*.png");
    request.r#match = MatchMode::Glob;
    request.share = Some("photos".to_owned());
    let result = server.search_files(Parameters(request)).await.unwrap();
    assert_eq!(links(&result).len(), 1);

    let mut request = search("");
    request.query = "a".to_owned();
    request.share = Some("photos".to_owned());
    request.kind = Some(KindParam::Image);
    assert_eq!(
        links(&server.search_files(Parameters(request)).await.unwrap()).len(),
        1
    );

    let mut request = search("");
    request.share = Some("photos".to_owned());
    request.modified_after = Some("2026-01-01".to_owned());
    assert_eq!(
        links(&server.search_files(Parameters(request)).await.unwrap()).len(),
        1
    );

    let mut request = search("");
    request.share = Some("photos".to_owned());
    request.modified_after = Some("2027-01-01".to_owned());
    assert_eq!(
        links(&server.search_files(Parameters(request)).await.unwrap()).len(),
        0
    );
}

#[tokio::test]
async fn searches_all_shares_without_share() {
    let (_dir, server, root) = setup();
    write(&root, "documents/find-me.txt", b"a");
    write(&root, "music/find-me.mp3", b"b");
    let result = server
        .search_files(Parameters(search("find-me")))
        .await
        .unwrap();
    let mut names: Vec<&str> = links(&result)
        .iter()
        .map(|link| link.name.as_str())
        .collect();
    names.sort();
    assert_eq!(names, ["find-me.mp3", "find-me.txt"]);
}

#[tokio::test]
async fn rejects_invalid_paths() {
    let (_dir, server, root) = setup();
    write(&root, "documents/a.txt", b"a");
    for path in [
        "/documents/a.txt",
        "documents/../a.txt",
        "documents/./a.txt",
        "documents//a.txt",
    ] {
        let result = server.read_text_file(Parameters(read(path))).await.unwrap();
        assert_error(&result, "invalid_path");
    }
    let result = server
        .list_directory(Parameters(ListDirectory {
            share: "documents".to_owned(),
            path: Some("/".to_owned()),
            offset: None,
            limit: None,
        }))
        .await
        .unwrap();
    assert_error(&result, "invalid_path");
}

#[tokio::test]
async fn subdir_needs_share() {
    let (_dir, server, _root) = setup();
    let mut request = search("a");
    request.subdir = Some("documents".to_owned());
    let result = server.search_files(Parameters(request)).await.unwrap();
    assert_error(&result, "invalid_param");
}

#[tokio::test]
async fn symlinks_are_not_followed() {
    let (_dir, server, root) = setup();
    write(&root, "documents/real.txt", b"real");
    symlink("real.txt", root.join("documents/link.txt")).unwrap();
    symlink("../documents", root.join("photos/album")).unwrap();

    let result = server
        .search_files(Parameters(search("real")))
        .await
        .unwrap();
    assert_eq!(links(&result).len(), 1);
    assert_eq!(links(&result)[0].name, "real.txt");

    let result = server
        .search_files(Parameters(search("link")))
        .await
        .unwrap();
    assert_eq!(links(&result).len(), 0);

    // 软链接目录不进去：photos 里没有别人的文件。
    let mut request = search("");
    request.share = Some("photos".to_owned());
    assert_eq!(
        links(&server.search_files(Parameters(request)).await.unwrap()).len(),
        0
    );

    let result = server
        .read_text_file(Parameters(read("link.txt")))
        .await
        .unwrap();
    assert_error(&result, "invalid_path");
    assert_eq!(structured(&result)["error"], "invalid_path");

    let listed = server
        .list_directory(Parameters(ListDirectory {
            share: "documents".to_owned(),
            path: None,
            offset: None,
            limit: None,
        }))
        .await
        .unwrap();
    let items = structured(&listed)["items"].as_array().unwrap();
    assert_eq!(
        items.iter().filter(|item| item["kind"] == "file").count(),
        1
    );
    assert_eq!(
        items.iter().filter(|item| item["kind"] == "other").count(),
        1
    );
    assert_eq!(links(&listed).len(), 1);
}

#[tokio::test]
async fn pages_at_most_100() {
    let (_dir, server, root) = setup();
    for index in 0..105 {
        write(&root, &format!("documents/f{index:03}.txt"), b"x");
    }
    let result = server.search_files(Parameters(search("f"))).await.unwrap();
    assert_eq!(structured(&result)["returned"], 50);
    assert_eq!(structured(&result)["total"], 105);

    let mut request = search("f");
    request.limit = Some(500);
    let result = server.search_files(Parameters(request)).await.unwrap();
    assert_eq!(structured(&result)["returned"], 100);

    let mut request = search("f");
    request.limit = Some(100);
    let result = server.search_files(Parameters(request)).await.unwrap();
    assert_eq!(structured(&result)["returned"], 100);
    assert_eq!(structured(&result)["next_offset"], 100);

    let mut request = search("f");
    request.offset = Some(100);
    let result = server.search_files(Parameters(request)).await.unwrap();
    assert_eq!(structured(&result)["returned"], 5);
    assert!(structured(&result)["next_offset"].is_null());
}

#[tokio::test]
async fn content_search_skips_binary_large_and_non_utf8() {
    let (_dir, server, root) = setup();
    write(&root, "documents/a.txt", b"NEEDLE");
    write(&root, "documents/bin.dat", b"NEEDLE\0rest");
    write(&root, "documents/gbk.txt", b"NEEDLE \xd6\xd0\xce\xc4");
    let mut big = vec![b'.'; 2 * 1024 * 1024];
    big.extend_from_slice(b"NEEDLE");
    write(&root, "documents/big.txt", &big);

    let result = server
        .search_content(Parameters(content("NEEDLE", None)))
        .await
        .unwrap();
    assert_eq!(links(&result)[0].name, "a.txt");
    assert_eq!(structured(&result)["notes"]["too_large"]["count"], 1);
    assert_eq!(structured(&result)["notes"]["binary"]["count"], 1);
    assert_eq!(structured(&result)["notes"]["not_utf8"]["count"], 1);

    let result = server
        .search_content(Parameters(content("NEEDLE", Some(4 * 1024 * 1024))))
        .await
        .unwrap();
    let names: Vec<&str> = links(&result)
        .iter()
        .map(|link| link.name.as_str())
        .collect();
    assert_eq!(names, ["a.txt", "big.txt"]);
}

#[tokio::test]
async fn reads_in_segments() {
    let (_dir, server, root) = setup();
    write(&root, "documents/long.txt", &vec![b'a'; 5000]);

    let result = server
        .read_text_file(Parameters(ReadTextFile {
            share: "documents".to_owned(),
            path: "long.txt".to_owned(),
            offset: None,
            limit: Some(1000),
        }))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(false));
    assert_eq!(structured(&result)["text"], "a".repeat(1000));
    assert_eq!(structured(&result)["next_offset"], 1000);

    let result = server
        .read_text_file(Parameters(ReadTextFile {
            share: "documents".to_owned(),
            path: "long.txt".to_owned(),
            offset: Some(5000),
            limit: None,
        }))
        .await
        .unwrap();
    assert!(structured(&result)["next_offset"].is_null());

    // limit 比一个汉字还短时也至少前进一个字符。
    write(&root, "documents/zh.txt", "中文".as_bytes());
    let result = server
        .read_text_file(Parameters(ReadTextFile {
            share: "documents".to_owned(),
            path: "zh.txt".to_owned(),
            offset: None,
            limit: Some(1),
        }))
        .await
        .unwrap();
    assert_eq!(structured(&result)["text"], "中");
    assert_eq!(structured(&result)["next_offset"], 3);
}

#[tokio::test]
async fn read_errors_are_distinguished() {
    let (_dir, server, root) = setup();
    write(&root, "documents/bin.dat", b"\x00\x01");
    write(&root, "documents/gbk.txt", b"\xd6\xd0\xce\xc4");
    write(&root, "documents/empty.txt", b"");

    let result = server
        .read_text_file(Parameters(read("bin.dat")))
        .await
        .unwrap();
    assert_error(&result, "not_text");
    let result = server
        .read_text_file(Parameters(read("gbk.txt")))
        .await
        .unwrap();
    assert_error(&result, "not_utf8");

    let empty = server
        .read_text_file(Parameters(read("empty.txt")))
        .await
        .unwrap();
    assert_eq!(structured(&empty)["size"], 0);
    assert_eq!(structured(&empty)["text"], "");

    let result = server
        .read_text_file(Parameters(ReadTextFile {
            share: "documents".to_owned(),
            path: "empty.txt".to_owned(),
            offset: Some(1),
            limit: None,
        }))
        .await
        .unwrap();
    assert_error(&result, "invalid_param");

    let result = server
        .read_text_file(Parameters(read("missing.txt")))
        .await
        .unwrap();
    assert_error(&result, "not_found");

    let result = server
        .read_text_file(Parameters(ReadTextFile {
            share: "nope".to_owned(),
            path: "a".to_owned(),
            offset: None,
            limit: None,
        }))
        .await
        .unwrap();
    assert_error(&result, "invalid_param");
}

#[tokio::test]
async fn invalid_utf8_at_the_boundary_is_an_error() {
    let (_dir, server, root) = setup();
    write(&root, "documents/bad.txt", b"\xe4AA");
    let result = server
        .read_text_file(Parameters(ReadTextFile {
            share: "documents".to_owned(),
            path: "bad.txt".to_owned(),
            offset: None,
            limit: Some(1),
        }))
        .await
        .unwrap();
    assert_error(&result, "not_utf8");
}

#[tokio::test]
async fn permission_denied_is_reported() {
    let (_dir, server, root) = setup();
    let secret = write(&root, "documents/secret/x.txt", b"x");
    let secret_dir = secret.parent().unwrap().to_owned();
    fs::set_permissions(&secret_dir, fs::Permissions::from_mode(0o000)).unwrap();

    let result = server.search_files(Parameters(search("x"))).await.unwrap();
    assert_eq!(structured(&result)["notes"]["denied"]["count"], 1);

    let result = server
        .list_directory(Parameters(ListDirectory {
            share: "documents".to_owned(),
            path: Some("secret".to_owned()),
            offset: None,
            limit: None,
        }))
        .await
        .unwrap();
    assert_error(&result, "denied");

    fs::set_permissions(&secret_dir, fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test]
async fn lists_directories_and_files() {
    let (_dir, server, root) = setup();
    write(&root, "documents/sub/a.txt", b"a");

    let result = server
        .list_directory(Parameters(ListDirectory {
            share: "documents".to_owned(),
            path: None,
            offset: None,
            limit: None,
        }))
        .await
        .unwrap();
    assert_eq!(structured(&result)["total"], 1);
    assert_eq!(structured(&result)["items"][0]["kind"], "dir");
    assert_eq!(links(&result).len(), 0);

    let result = server
        .list_directory(Parameters(ListDirectory {
            share: "documents".to_owned(),
            path: Some("sub".to_owned()),
            offset: None,
            limit: None,
        }))
        .await
        .unwrap();
    assert_eq!(links(&result).len(), 1);
    assert_eq!(links(&result)[0].uri, "wonton-share:documents/sub/a.txt");
    assert_eq!(structured(&result)["items"][0]["kind"], "file");
    assert_eq!(
        structured(&result)["items"][0]["uri"],
        "wonton-share:documents/sub/a.txt"
    );
}

#[tokio::test]
async fn every_tool_is_read_only() {
    let (_dir, server, _root) = setup();
    let tools = server.tool_defs();
    let mut names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "list_directory",
            "read_text_file",
            "search_content",
            "search_files"
        ]
    );
    for tool in tools {
        assert_eq!(
            tool.annotations.as_ref().unwrap().read_only_hint,
            Some(true),
            "{}",
            tool.name
        );
    }
}

#[tokio::test]
async fn read_realigns_offset_inside_a_character() {
    let (_dir, server, root) = setup();
    write(&root, "documents/zh.txt", "中文".as_bytes());
    let result = server
        .read_text_file(Parameters(ReadTextFile {
            share: "documents".to_owned(),
            path: "zh.txt".to_owned(),
            offset: Some(1),
            limit: Some(8),
        }))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(false));
    assert_eq!(structured(&result)["text"], "文");
    assert_eq!(structured(&result)["start"], 3);
    assert!(structured(&result)["next_offset"].is_null());
}

#[tokio::test]
async fn non_utf8_names_are_returned_with_encoded_uri() {
    let (_dir, server, root) = setup();
    let name = OsStr::from_bytes(b"bad\xff name.txt");
    fs::write(root.join("documents").join(name), b"x").unwrap();
    let result = server
        .search_files(Parameters(search("bad")))
        .await
        .unwrap();
    let links = links(&result);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].uri, "wonton-share:documents/bad%FF%20name.txt");
    assert_eq!(structured(&result)["notes"]["non_utf8"]["count"], 1);
}

#[tokio::test]
async fn reports_directories_deeper_than_64() {
    let (_dir, server, root) = setup();
    let mut path = PathBuf::from("documents");
    for index in 0..70 {
        path.push(format!("d{index:02}"));
    }
    fs::create_dir_all(root.join(&path)).unwrap();
    fs::write(root.join(&path).join("deep.txt"), b"x").unwrap();
    let result = server
        .search_files(Parameters(search("deep")))
        .await
        .unwrap();
    assert_eq!(links(&result).len(), 0);
    assert_eq!(structured(&result)["notes"]["too_deep"]["count"], 1);
}

#[tokio::test]
async fn reports_the_entry_budget() {
    let (_dir, _server, root) = setup();
    for index in 0..20 {
        write(&root, &format!("documents/f{index:02}.txt"), b"x");
    }
    let server = wonton_files::FilesServer::with_limits(
        &root,
        Limits {
            entries: 5,
            ..Limits::default()
        },
    );
    let result = server.search_files(Parameters(search("f"))).await.unwrap();
    assert_eq!(structured(&result)["stopped"], true);
}

#[tokio::test]
async fn content_search_reports_too_large_and_unreadable() {
    let (_dir, server, root) = setup();
    write(&root, "documents/big.txt", &[b'a'; 20]);
    let unreadable = write(&root, "documents/secret.txt", b"a");
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();

    let result = server
        .search_content(Parameters(content("a", Some(10))))
        .await
        .unwrap();
    assert_eq!(structured(&result)["notes"]["too_large"]["count"], 1);
    assert_eq!(structured(&result)["notes"]["unreadable"]["count"], 1);

    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644)).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn health_answers_while_a_slow_search_runs() {
    let (_dir, server, root) = setup();
    write(&root, "documents/slow.txt", b"x");
    let server = server.with_delay(Duration::from_millis(500));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = wonton_files::app(
        server.clone(),
        rmcp::transport::streamable_http_server::StreamableHttpServerConfig::default(),
    );
    let serving = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let search = tokio::spawn(async move {
        server
            .search_files(Parameters(search("slow")))
            .await
            .unwrap()
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let health = tokio::time::timeout(Duration::from_millis(300), async {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    })
    .await
    .expect("healthz 被慢搜索挡住了");
    assert!(health.starts_with("HTTP/1.1 200"), "{health}");

    assert_eq!(links(&search.await.unwrap()).len(), 1);
    serving.abort();
}

#[tokio::test]
async fn health_reads_each_share() {
    let (_dir, _server, root) = setup();
    write(&root, "documents/a.txt", b"a");
    assert!(wonton_files::health(&root).is_ok());

    fs::set_permissions(root.join("documents"), fs::Permissions::from_mode(0o000)).unwrap();
    let err = wonton_files::health(&root).unwrap_err();
    assert!(err.contains("documents"));
    fs::set_permissions(root.join("documents"), fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test]
async fn directory_listing_stops_at_the_entry_limit() {
    let (_dir, _server, root) = setup();
    for index in 0..10 {
        write(&root, &format!("documents/f{index:02}.txt"), b"x");
    }
    let server = wonton_files::FilesServer::with_limits(
        &root,
        Limits {
            entries: 5,
            ..Limits::default()
        },
    );
    let result = server
        .list_directory(Parameters(ListDirectory {
            share: "documents".to_owned(),
            path: None,
            offset: None,
            limit: None,
        }))
        .await
        .unwrap();
    assert_eq!(links(&result).len(), 5);
    assert_eq!(structured(&result)["stopped"], true);
}
