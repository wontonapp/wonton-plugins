use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use tempfile::TempDir;
use wonton_files::tools::{MatchMode, ReadTextFile, SearchContent, SearchFiles};
use wonton_files::{FilesServer, SHARES};

fn setup() -> (TempDir, FilesServer) {
    let dir = tempfile::tempdir().unwrap();
    for share in SHARES {
        fs::create_dir(dir.path().join(share)).unwrap();
    }
    let server = FilesServer::new(dir.path());
    (dir, server)
}

fn read(path: &str, offset: u64, limit: u64) -> ReadTextFile {
    ReadTextFile {
        share: "documents".to_owned(),
        path: path.to_owned(),
        offset: Some(offset),
        limit: Some(limit),
    }
}

fn assert_error(result: &CallToolResult, code: &str) {
    assert_eq!(result.is_error, Some(true));
    assert_eq!(result.structured_content.as_ref().unwrap()["error"], code);
}

#[tokio::test]
async fn invalid_continuation_prefixes_are_not_skipped() {
    let (dir, server) = setup();
    for (bytes, offset) in [
        (vec![0x80; 8], 0),
        (b"\x80\x80hello".to_vec(), 0),
        (b"\x80\x80hello".to_vec(), 1),
        (b"a\x80hello".to_vec(), 1),
        (b"\xe4\x80Ahello".to_vec(), 1),
        (b"\xf0\x80\x80\x80hello".to_vec(), 2),
    ] {
        fs::write(dir.path().join("documents/bad.txt"), bytes).unwrap();
        let result = server
            .read_text_file(Parameters(read("bad.txt", offset, 64)))
            .await
            .unwrap();
        assert_error(&result, "not_utf8");
    }
}

#[tokio::test]
async fn middle_offsets_require_a_valid_complete_character() {
    let (dir, server) = setup();
    for character in ["é", "中", "🗂"] {
        let body = format!("abc{character}tail");
        fs::write(dir.path().join("documents/valid.txt"), body.as_bytes()).unwrap();
        for byte in 1..character.len() {
            let result = server
                .read_text_file(Parameters(read("valid.txt", (3 + byte) as u64, 64)))
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(false));
            let data = result.structured_content.unwrap();
            assert_eq!(data["start"], 3 + character.len());
            assert_eq!(data["text"], "tail");
            assert!(data["next_offset"].is_null());
        }
    }
}

#[tokio::test]
async fn bad_dates_and_empty_queries_are_parameter_errors() {
    let (_dir, server) = setup();
    let result = server
        .search_files(Parameters(SearchFiles {
            query: String::new(),
            share: Some("documents".to_owned()),
            subdir: None,
            r#match: MatchMode::Substring,
            case_sensitive: false,
            modified_after: Some("2026-02-31".to_owned()),
            kind: None,
            offset: None,
            limit: None,
        }))
        .await
        .unwrap();
    assert_error(&result, "invalid_param");

    let result = server
        .search_content(Parameters(SearchContent {
            query: String::new(),
            share: Some("documents".to_owned()),
            subdir: None,
            modified_after: None,
            kind: None,
            max_size: None,
            offset: None,
            limit: None,
        }))
        .await
        .unwrap();
    assert_error(&result, "invalid_param");
    let result = server
        .read_text_file(Parameters(read("../outside", 0, 64)))
        .await
        .unwrap();
    assert_error(&result, "invalid_path");
}

#[test]
fn reads_a_fifo_without_blocking() {
    let (dir, _server) = setup();
    rustix::fs::mknodat(
        rustix::fs::CWD,
        dir.path().join("documents/pipe"),
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "fifo_child"])
        .env("WONTON_FIFO_TEST_ROOT", dir.path())
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "FIFO child failed: {status}");
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("FIFO read exceeded the child process deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "run by reads_a_fifo_without_blocking in a killable child process"]
fn fifo_child() {
    let root = PathBuf::from(std::env::var_os("WONTON_FIFO_TEST_ROOT").unwrap());
    let server = FilesServer::new(root);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(server.read_text_file(Parameters(read("pipe", 0, 64))))
        .unwrap();
    assert_error(&result, "invalid_path");
}
