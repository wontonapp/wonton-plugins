//! 对 Jellyfin 的 HTTP 调用。两种身份：管理员 API key（建人、批 Quick Connect、删设备）和每个人自己的
//! 令牌（列片、播放会话、进度都记在本人名下）。凭据只放 `Authorization` 头，不进 URL：Jellyfin 也收
//! `ApiKey=` 查询参数，但那样凭据会跟着地址进日志和播放列表。

use reqwest::{Method, RequestBuilder, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::failure::{Failure, Reply};

const CLIENT: &str = "Wonton";
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 一个调用身份：`device` 是 Jellyfin 的设备号，Quick Connect 发的令牌绑在它上面。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub device: String,
    pub token: String,
}

impl Identity {
    fn header(&self) -> String {
        format!(
            r#"MediaBrowser Client="{CLIENT}", Device="{CLIENT}", DeviceId="{}", Version="{VERSION}", Token="{}""#,
            self.device, self.token
        )
    }
}

#[derive(Clone)]
pub struct Jellyfin {
    base: String,
    http: reqwest::Client,
}

impl Jellyfin {
    pub fn new(base: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("reqwest 客户端用默认设置建得起来");
        Self {
            base: base.into().trim_end_matches('/').to_owned(),
            http,
        }
    }

    pub fn request(&self, method: Method, path_and_query: &str, who: &Identity) -> RequestBuilder {
        self.http
            .request(method, format!("{}{path_and_query}", self.base))
            .header(reqwest::header::AUTHORIZATION, who.header())
    }

    /// 发出去，非 2xx 折成 [`Failure`]；`what` 是给人看的那一步。
    pub async fn send(&self, req: RequestBuilder, what: &str) -> Reply<Response> {
        let resp = req
            .send()
            .await
            .map_err(|e| Failure::unreachable(what, &e))?;
        if resp.status().is_success() {
            return Ok(resp);
        }
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());
        Err(Failure::from_upstream(
            resp.status().as_u16(),
            what,
            retry_after,
        ))
    }

    pub async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        who: &Identity,
        what: &str,
    ) -> Reply<T> {
        let resp = self
            .send(self.request(Method::GET, path, who), what)
            .await?;
        decode(resp, what).await
    }

    pub async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
        who: &Identity,
        what: &str,
    ) -> Reply<T> {
        let req = self.request(Method::POST, path, who).json(body);
        decode(self.send(req, what).await?, what).await
    }

    /// 只要成功、不要回体的调用（Jellyfin 这类接口回 204）。
    pub async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        who: &Identity,
        what: &str,
    ) -> Reply<()> {
        let mut req = self.request(method, path, who);
        if let Some(body) = body {
            req = req.json(body);
        }
        self.send(req, what).await.map(|_| ())
    }
}

async fn decode<T: DeserializeOwned>(resp: Response, what: &str) -> Reply<T> {
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| Failure::unreachable(what, &e))?;
    serde_json::from_slice(&bytes).map_err(|e| {
        Failure::new(
            crate::failure::Code::Protocol,
            format!("{what}：Jellyfin 的回体解析不了（{e}）"),
        )
    })
}

/// Jellyfin 的时长单位是 100 纳秒（tick）。
pub fn ticks_to_ms(ticks: u64) -> u64 {
    ticks / 10_000
}

pub fn ms_to_ticks(ms: u64) -> u64 {
    ms.saturating_mul(10_000)
}

/// 查询串里的值按 `application/x-www-form-urlencoded` 编码；搜索词里有中文和空格。
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_carries_device_and_token() {
        let who = Identity {
            device: "dev-1".into(),
            token: "t0k".into(),
        };
        let h = who.header();
        assert!(h.starts_with("MediaBrowser Client=\"Wonton\""));
        assert!(h.contains("DeviceId=\"dev-1\""));
        assert!(h.contains("Token=\"t0k\""));
    }

    #[test]
    fn ticks_round_trip_in_ms() {
        assert_eq!(ticks_to_ms(300_000_000), 30_000);
        assert_eq!(ms_to_ticks(30_000), 300_000_000);
    }

    #[test]
    fn query_values_are_percent_encoded() {
        assert_eq!(encode("沙丘 2"), "%E6%B2%99%E4%B8%98%202");
        assert_eq!(encode("a&b=c"), "a%26b%3Dc");
    }
}
