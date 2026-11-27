//! 上传（契约的 `upload` 操作）：核心按 tus 的样子分片续写，Immich 只收整个文件（`POST /assets` 是一次
//! multipart），所以字节先落在状态卷里，收尾时一次交给 Immich。断点就是落盘文件的长度。

use std::path::{Path, PathBuf};

use axum::Json;
use axum::body::Body;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use futures_util::StreamExt;
use reqwest::Method;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;

use crate::App;
use crate::failure::{Failure, JsonBody, Result};
use crate::immich::Auth;
use crate::store;

pub const OFFSET_HEADER: &str = "upload-offset";
const SHA1_PREFIX: &str = "sha1:";
/// 一次检查最多几个校验和（契约 `UPLOAD_CHECK_MAX`）。
const CHECK_MAX: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadStart {
    pub path: String,
    pub size: u64,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub taken_at: Option<u64>,
    #[serde(default)]
    pub live_video_checksum: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UploadStarted {
    pub upload_id: String,
    pub upload_offset: u64,
}

#[derive(Debug, Serialize)]
pub struct UploadFinished {
    pub item_id: String,
    pub duplicate: bool,
}

#[derive(Debug, Deserialize)]
pub struct UploadCheck {
    pub checksums: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ExistingItem {
    pub checksum: String,
    pub item_id: String,
}

fn dir(app: &App, user: &str, id: &str) -> Result<PathBuf> {
    let valid = id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        return Err(Failure::gone(format!("没有这个上传：{id}")));
    }
    Ok(app.state.join("uploads").join(user).join(id))
}

fn read_meta(dir: &Path) -> Result<UploadStart> {
    match std::fs::read(dir.join("meta.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|err| Failure::upstream(format!("上传记录坏了：{err}"))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Err(Failure::gone("没有这个上传（已收尾、已放弃或从没建过）"))
        }
        Err(err) => Err(Failure::io("读上传记录", &err)),
    }
}

fn received(dir: &Path) -> Result<u64> {
    std::fs::metadata(dir.join("data"))
        .map(|meta| meta.len())
        .map_err(|err| Failure::io("读已收字节", &err))
}

/// sha1 的 hex；不是 `sha1:` 开头的（别的算法）返回空，Immich 只认 SHA-1。
fn sha1_hex(checksum: &str) -> Option<&str> {
    checksum
        .strip_prefix(SHA1_PREFIX)
        .filter(|hex| hex.len() == 40 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

pub async fn start(
    State(app): State<App>,
    headers: HeaderMap,
    JsonBody(start): JsonBody<UploadStart>,
) -> Result<Json<UploadStarted>> {
    let account = app.person(&headers).await?;
    if start.path.trim().is_empty() || start.size == 0 {
        return Err(Failure::protocol("上传要有路径，大小不能是 0"));
    }
    // Immich 的时间线按拍摄时间排；没有就拒，不拿上传时刻顶上，免得旧照片全堆在今天。
    if start.taken_at.is_none() {
        return Err(Failure::protocol("照片要带拍摄时间（taken_at）"));
    }
    for checksum in [&start.checksum, &start.live_video_checksum]
        .into_iter()
        .flatten()
    {
        if sha1_hex(checksum).is_none() {
            return Err(Failure::protocol(format!(
                "校验和要写成 sha1:<40 位 hex>：{checksum}"
            )));
        }
    }
    let id = store::random_hex(16);
    let dir = dir(&app, &account.user, &id)?;
    std::fs::create_dir_all(&dir).map_err(|err| Failure::io("建上传目录", &err))?;
    std::fs::File::create(dir.join("data")).map_err(|err| Failure::io("建上传文件", &err))?;
    let meta = serde_json::to_vec(&start).expect("上传记录能序列化");
    store::write_private(&dir.join("meta.json"), &meta)
        .map_err(|err| Failure::io("写上传记录", &err))?;
    Ok(Json(UploadStarted {
        upload_id: id,
        upload_offset: 0,
    }))
}

fn offset_response(status: StatusCode, offset: u64) -> Response {
    let mut response = (status, Json(json!({"upload_offset": offset}))).into_response();
    response
        .headers_mut()
        .insert(OFFSET_HEADER, HeaderValue::from(offset));
    response
}

pub async fn head(
    State(app): State<App>,
    headers: HeaderMap,
    UrlPath(id): UrlPath<String>,
) -> Result<Response> {
    let account = app.person(&headers).await?;
    let dir = dir(&app, &account.user, &id)?;
    read_meta(&dir)?;
    Ok(offset_response(StatusCode::OK, received(&dir)?))
}

/// 只认从当前偏移起的续写：`Upload-Offset` 不等于已收字节回 409 带当前偏移，核心按它重来。
pub async fn patch(
    State(app): State<App>,
    headers: HeaderMap,
    UrlPath(id): UrlPath<String>,
    body: Body,
) -> Result<Response> {
    let account = app.person(&headers).await?;
    let dir = dir(&app, &account.user, &id)?;
    let meta = read_meta(&dir)?;
    let have = received(&dir)?;
    let claimed = headers
        .get(OFFSET_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| Failure::protocol("续写要带 Upload-Offset"))?;
    if claimed != have {
        return Ok(offset_response(StatusCode::CONFLICT, have));
    }
    let mut file = tokio::fs::OpenOptions::new()
        .append(true)
        .open(dir.join("data"))
        .await
        .map_err(|err| Failure::io("打开上传文件", &err))?;
    let mut total = have;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| Failure::protocol(format!("请求体读断了：{err}")))?;
        total += chunk.len() as u64;
        if total > meta.size {
            return Err(Failure::protocol(format!(
                "写过头了：声明 {} 字节，收到 {total}",
                meta.size
            )));
        }
        file.write_all(&chunk)
            .await
            .map_err(|err| Failure::io("写上传文件", &err))?;
    }
    file.flush()
        .await
        .map_err(|err| Failure::io("写上传文件", &err))?;
    Ok(offset_response(StatusCode::OK, total))
}

pub async fn delete(
    State(app): State<App>,
    headers: HeaderMap,
    UrlPath(id): UrlPath<String>,
) -> Result<StatusCode> {
    let account = app.person(&headers).await?;
    let dir = dir(&app, &account.user, &id)?;
    match std::fs::remove_dir_all(&dir) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            Err(Failure::io("删上传目录", &err))
        }
        _ => Ok(StatusCode::NO_CONTENT),
    }
}

fn file_sha1(path: &Path) -> Result<[u8; 20]> {
    let mut file = std::fs::File::open(path).map_err(|err| Failure::io("读上传文件", &err))?;
    let mut hasher = Sha1::new();
    std::io::copy(&mut file, &mut hasher).map_err(|err| Failure::io("读上传文件", &err))?;
    Ok(hasher.finalize().into())
}

fn rfc3339(ms: Option<u64>) -> Result<String> {
    ms.and_then(|ms| chrono::DateTime::from_timestamp_millis(ms as i64))
        .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .ok_or_else(|| Failure::protocol(format!("拍摄时间不对：{ms:?}")))
}

pub async fn finish(
    State(app): State<App>,
    headers: HeaderMap,
    UrlPath(id): UrlPath<String>,
) -> Result<Json<UploadFinished>> {
    let account = app.person(&headers).await?;
    let dir = dir(&app, &account.user, &id)?;
    let meta = read_meta(&dir)?;
    let have = received(&dir)?;
    if have != meta.size {
        return Err(Failure::protocol(format!(
            "还没传完：声明 {} 字节，收到 {have}",
            meta.size
        )));
    }
    let digest = file_sha1(&dir.join("data"))?;
    if let Some(expected) = meta.checksum.as_deref().and_then(sha1_hex)
        && !expected.eq_ignore_ascii_case(&hex::encode(digest))
    {
        // 字节坏了：丢掉这次上传，核心从头再来，不把坏文件交给 Immich。
        if let Err(err) = std::fs::remove_dir_all(&dir) {
            eprintln!("删坏掉的暂存失败（{}）：{err}", dir.display());
        }
        return Err(Failure::protocol("收到的字节和校验和对不上，这次上传作废"));
    }
    let live = match meta.live_video_checksum.as_deref().and_then(sha1_hex) {
        Some(hex) => Some(live_video_id(&app, &account.api_key, hex).await?),
        None => None,
    };
    let reply = upload_to_immich(&app, &account.api_key, &dir, &meta, &digest, live).await;
    let reply = reply.map_err(|err| app.on_immich_error(&account.user, err))?;
    if let Err(err) = std::fs::remove_dir_all(&dir) {
        eprintln!("已交给 Immich，删暂存失败（{}）：{err}", dir.display());
    }
    Ok(Json(reply))
}

async fn upload_to_immich(
    app: &App,
    key: &str,
    dir: &Path,
    meta: &UploadStart,
    digest: &[u8; 20],
    live: Option<String>,
) -> Result<UploadFinished> {
    let name = meta
        .path
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("upload")
        .to_owned();
    let file = tokio::fs::File::open(dir.join("data"))
        .await
        .map_err(|err| Failure::io("读上传文件", &err))?;
    let stream = tokio_util::io::ReaderStream::new(file);
    let part = Part::stream_with_length(reqwest::Body::wrap_stream(stream), meta.size)
        .file_name(name.clone());
    let at = rfc3339(meta.taken_at)?;
    let mut form = Form::new()
        .part("assetData", part)
        .text("fileCreatedAt", at.clone())
        .text("fileModifiedAt", at)
        .text("filename", name);
    if let Some(live) = live {
        form = form.text("livePhotoVideoId", live);
    }
    let checksum = base64::engine::general_purpose::STANDARD.encode(digest);
    let builder = app
        .immich
        .request(Method::POST, "/assets", Auth::Key(key))
        .header("x-immich-checksum", checksum)
        .multipart(form);
    let response = app.immich.send(builder, "POST /assets").await?;
    let reply: Value = response
        .json()
        .await
        .map_err(|err| Failure::protocol(format!("Immich POST /assets 的回包对不上：{err}")))?;
    let item_id = reply["id"]
        .as_str()
        .ok_or_else(|| Failure::protocol("Immich POST /assets 没有回 id"))?;
    Ok(UploadFinished {
        item_id: item_id.to_owned(),
        duplicate: reply["status"] == "duplicate",
    })
}

/// 问 Immich 这些 SHA-1（hex）哪些已在这个人库里，回 hex → 条目 id。
async fn existing(app: &App, key: &str, hexes: &[&str]) -> Result<Vec<(String, String)>> {
    let assets: Vec<Value> = hexes
        .iter()
        .map(|hex| json!({"id": hex, "checksum": hex}))
        .collect();
    let body = json!({"assets": assets});
    let reply: Value = app
        .immich
        .json(
            Method::POST,
            "/assets/bulk-upload-check",
            Auth::Key(key),
            Some(&body),
        )
        .await?;
    let results = reply["results"]
        .as_array()
        .ok_or_else(|| Failure::protocol("Immich bulk-upload-check 没有 results"))?;
    Ok(results
        .iter()
        .filter(|r| r["action"] == "reject" && r["reason"] == "duplicate")
        .filter_map(|r| {
            Some((
                r["id"].as_str()?.to_owned(),
                r["assetId"].as_str()?.to_owned(),
            ))
        })
        .collect())
}

/// 实况照片的动态那段：同一任务里排在前面先传完，这里按校验和找回它在 Immich 的 id。
async fn live_video_id(app: &App, key: &str, hex: &str) -> Result<String> {
    existing(app, key, &[hex])
        .await?
        .into_iter()
        .next()
        .map(|(_, id)| id)
        .ok_or_else(|| Failure::protocol("实况照片的动态部分还不在库里：它要先传完"))
}

pub async fn check(
    State(app): State<App>,
    headers: HeaderMap,
    JsonBody(check): JsonBody<UploadCheck>,
) -> Result<Json<Value>> {
    let account = app.person(&headers).await?;
    if check.checksums.len() > CHECK_MAX {
        return Err(Failure::protocol(format!("一次最多问 {CHECK_MAX} 个")));
    }
    let hexes: Vec<&str> = check.checksums.iter().filter_map(|c| sha1_hex(c)).collect();
    if hexes.is_empty() {
        return Ok(Json(json!({"existing": []})));
    }
    let found = existing(&app, &account.api_key, &hexes)
        .await
        .map_err(|err| app.on_immich_error(&account.user, err))?;
    let existing: Vec<ExistingItem> = found
        .into_iter()
        .map(|(hex, item_id)| ExistingItem {
            checksum: format!("{SHA1_PREFIX}{}", hex.to_ascii_lowercase()),
            item_id,
        })
        .collect();
    Ok(Json(json!({"existing": existing})))
}
