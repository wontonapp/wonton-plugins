//! 字节：播放流、离线下载、封面。`Range`/`If-Range` 原样带给 Navidrome，状态码和分段头原样带回。

use axum::body::Body;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::failure::Reply;
use crate::playback::song;
use crate::upstream::{Params, Upstream};

const FORWARD_REQUEST: [header::HeaderName; 2] = [header::RANGE, header::IF_RANGE];
const FORWARD_RESPONSE: [header::HeaderName; 7] = [
    header::CONTENT_TYPE,
    header::CONTENT_LENGTH,
    header::CONTENT_RANGE,
    header::ACCEPT_RANGES,
    header::ETAG,
    header::LAST_MODIFIED,
    header::CACHE_CONTROL,
];

/// 播放流：不带 `format` 是原文件（`raw`），带了就让 Navidrome 转码，`max_bitrate` 是 kbps。
pub(crate) async fn stream(
    upstream: &Upstream,
    person: &str,
    id: &str,
    format: Option<&str>,
    max_bitrate: Option<u32>,
    request: &HeaderMap,
) -> Reply<Response> {
    let mut params: Params = vec![("id", id.to_owned())];
    match format {
        None | Some("raw") => params.push(("format", "raw".to_owned())),
        Some(format) => {
            params.push(("format", format.to_owned()));
            // 转码流默认没有长度，播放器拿不到时长和缓冲进度；让 Navidrome 按码率估一个。
            params.push(("estimateContentLength", "true".to_owned()));
        }
    }
    if let Some(kbps) = max_bitrate {
        params.push(("maxBitRate", kbps.to_string()));
    }
    relay(upstream, person, "stream", &params, request).await
}

pub(crate) async fn download(
    upstream: &Upstream,
    person: &str,
    id: &str,
    request: &HeaderMap,
) -> Reply<Response> {
    relay(
        upstream,
        person,
        "download",
        &vec![("id", id.to_owned())],
        request,
    )
    .await
}

/// 封面按歌的 `coverArt` 取；没有封面回 404，不当失败。
pub(crate) async fn artwork(
    upstream: &Upstream,
    person: &str,
    id: &str,
    size: Option<u32>,
) -> Reply<Response> {
    let song = song(upstream, person, id).await?;
    let Some(cover) = song.get("coverArt").and_then(Value::as_str) else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let mut params: Params = vec![("id", cover.to_owned())];
    if let Some(size) = size {
        params.push(("size", size.to_string()));
    }
    relay(upstream, person, "getCoverArt", &params, &HeaderMap::new()).await
}

async fn relay(
    upstream: &Upstream,
    person: &str,
    method: &str,
    params: &Params,
    request: &HeaderMap,
) -> Reply<Response> {
    let mut forward = HeaderMap::new();
    for name in FORWARD_REQUEST {
        if let Some(value) = request.get(&name) {
            forward.insert(name, value.clone());
        }
    }
    let reply = upstream.fetch(person, method, params, &forward).await?;
    let mut response = Response::builder().status(reply.status());
    for name in FORWARD_RESPONSE {
        if let Some(value) = reply.headers().get(&name) {
            response = response.header(name, value.clone());
        }
    }
    Ok(response
        .body(Body::from_stream(reply.bytes_stream()))
        .expect("status and headers come from a valid upstream response"))
}
