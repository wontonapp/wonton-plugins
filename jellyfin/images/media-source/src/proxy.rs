//! 字节原样转发：直链、HLS 播放列表与分片、离线下载、海报。凭据在这里用请求头补上，URL 里不带；
//! `Range`/`If-Range` 原样过，Jellyfin 回 206 就回 206。流式透传，不缓冲、不落盘。

use axum::body::Body;
use axum::http::{HeaderMap, HeaderName, StatusCode, header};
use axum::response::{IntoResponse, Response};
use reqwest::Method;

use crate::accounts::Account;
use crate::failure::{Code, Failure, Reply};
use crate::jellyfin::Jellyfin;
use crate::playback::strip_credentials;

/// 播放列表要整份读进来改写，限个大小；几小时的片子分片列表也就几百 KiB。
const MAX_PLAYLIST_BYTES: usize = 4 << 20;

/// 不转 `If-None-Match`/`If-Modified-Since`：304 没有回体，契约里也没有这种回法。
const PASS_REQUEST: &[HeaderName] = &[header::RANGE, header::IF_RANGE];

const PASS_RESPONSE: &[HeaderName] = &[
    header::CONTENT_TYPE,
    header::CONTENT_LENGTH,
    header::CONTENT_RANGE,
    header::ACCEPT_RANGES,
    header::ETAG,
    header::LAST_MODIFIED,
    header::CONTENT_DISPOSITION,
    header::CACHE_CONTROL,
];

pub async fn forward(
    jf: &Jellyfin,
    who: &Account,
    path_and_query: &str,
    incoming: &HeaderMap,
    what: &str,
) -> Reply<Response> {
    let (status, headers, upstream) = open(jf, who, path_and_query, incoming, what).await?;
    Ok((status, headers, Body::from_stream(upstream.bytes_stream())).into_response())
}

/// HLS 播放列表：Jellyfin 会把请求方的令牌以 `ApiKey=` 写进字幕轨等地址（12.4 实测），原样转出去令牌就到了手机上。
/// 逐行剥掉，播放器按相对地址回来时由 [`forward`] 在请求头里补。
pub async fn forward_playlist(
    jf: &Jellyfin,
    who: &Account,
    path_and_query: &str,
    incoming: &HeaderMap,
    what: &str,
) -> Reply<Response> {
    let (status, mut headers, upstream) = open(jf, who, path_and_query, incoming, what).await?;
    let bytes = upstream
        .bytes()
        .await
        .map_err(|e| Failure::unreachable(what, &e))?;
    if bytes.len() > MAX_PLAYLIST_BYTES {
        return Err(Failure::new(
            Code::Protocol,
            format!("{what}：播放列表超过 4 MiB"),
        ));
    }
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|_| Failure::new(Code::Protocol, format!("{what}：播放列表不是 UTF-8")))?;
    let body = scrub_playlist(&text);
    headers.remove(header::CONTENT_LENGTH);
    Ok((status, headers, body).into_response())
}

pub fn scrub_playlist(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        if line.starts_with('#') {
            out.push_str(&scrub_uri_attribute(line));
        } else if line.trim().is_empty() {
            out.push_str(line);
        } else {
            out.push_str(&strip_credentials(line));
        }
        out.push('\n');
    }
    out
}

fn scrub_uri_attribute(line: &str) -> String {
    let Some(start) = line.find("URI=\"") else {
        return line.to_owned();
    };
    let value_start = start + "URI=\"".len();
    let Some(len) = line[value_start..].find('"') else {
        return line.to_owned();
    };
    let value = &line[value_start..value_start + len];
    format!(
        "{}{}{}",
        &line[..value_start],
        strip_credentials(value),
        &line[value_start + len..]
    )
}

async fn open(
    jf: &Jellyfin,
    who: &Account,
    path_and_query: &str,
    incoming: &HeaderMap,
    what: &str,
) -> Reply<(StatusCode, HeaderMap, reqwest::Response)> {
    let mut req = jf.request(Method::GET, path_and_query, &who.identity());
    for name in PASS_REQUEST {
        if let Some(value) = incoming.get(name) {
            req = req.header(name.as_str(), value.as_bytes());
        }
    }
    let upstream = jf.send(req, what).await?;
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut headers = HeaderMap::new();
    for name in PASS_RESPONSE {
        if let Some(value) = upstream.headers().get(name.as_str())
            && let Ok(value) = axum::http::HeaderValue::from_bytes(value.as_bytes())
        {
            headers.insert(name.clone(), value);
        }
    }
    Ok((status, headers, upstream))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlists_lose_their_tokens_but_keep_relative_paths() {
        let master = "#EXTM3U\n#EXT-X-MEDIA:TYPE=SUBTITLES,URI=\"abc/Subtitles/0/subtitles.m3u8?SegmentLength=30&ApiKey=secret\",LANGUAGE=\"zho\"\nmain.m3u8?DeviceId=d&api_key=secret&Tag=t\n";
        let got = scrub_playlist(master);
        assert!(!got.contains("secret"));
        assert!(
            got.contains(
                "URI=\"abc/Subtitles/0/subtitles.m3u8?SegmentLength=30\",LANGUAGE=\"zho\""
            )
        );
        assert!(got.contains("\nmain.m3u8?DeviceId=d&Tag=t\n"));
    }

    #[test]
    fn lines_without_tokens_are_untouched() {
        let media = "#EXTM3U\n#EXTINF:6.0,\nhls1/main/0.ts?x=1\n#EXT-X-ENDLIST\n";
        assert_eq!(scrub_playlist(media), media);
    }
}
