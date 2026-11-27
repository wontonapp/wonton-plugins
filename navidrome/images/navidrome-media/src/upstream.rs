//! 讲 `OpenSubsonic` 的那一侧：每个请求带 `Remote-User`，不带 `u`/`p`。
//!
//! Navidrome 0.64.2 的实测行为：白名单来源带这个头时 `/rest/*` 不要口令；头里的人还没有账号时回错误 40，
//! 带头打开一次网页（`GET /app/`）就会建号。所以 40 先建号再重试一次，还是 40 才算没登录。

use axum::http::{HeaderMap, StatusCode};
use serde_json::Value;

use crate::failure::{MediaFailure, Reply};

pub const USER_HEADER: &str = "Remote-User";
const API_VERSION: &str = "1.16.1";
const CLIENT_NAME: &str = "wonton-media";
/// Subsonic 的错误码（<https://opensubsonic.netlify.app/docs/responses/error/>）。
const WRONG_CREDENTIALS: i64 = 40;
const NOT_AUTHORIZED: i64 = 50;
const NOT_FOUND: i64 = 70;

pub type Params = Vec<(&'static str, String)>;

#[derive(Clone)]
pub struct Upstream {
    base: String,
    http: reqwest::Client,
}

enum Attempt<T> {
    Done(T),
    NoAccount,
}

impl Upstream {
    /// `base` 是 Navidrome 的根地址，如 `http://127.0.0.1:4533`。
    ///
    /// # Panics
    ///
    /// 不会：没有 TLS、不读代理设置的客户端总能建出来。
    #[must_use]
    pub fn new(base: &str) -> Self {
        let http = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("reqwest client without TLS or proxy always builds");
        Self {
            base: base.trim_end_matches('/').to_owned(),
            http,
        }
    }

    /// 调一个回 JSON 的方法，回 `subsonic-response` 那一层。
    ///
    /// # Errors
    ///
    /// 连不上、上游回 HTTP 错误或 Subsonic 失败信封时，回归一后的 [`MediaFailure`]。
    pub async fn call(&self, person: &str, method: &str, params: &Params) -> Reply<Value> {
        if let Attempt::Done(value) = self.call_once(person, method, params).await? {
            return Ok(value);
        }
        self.open_web(person).await?;
        match self.call_once(person, method, params).await? {
            Attempt::Done(value) => Ok(value),
            Attempt::NoAccount => Err(no_account(person)),
        }
    }

    /// 调一个回字节的方法（`stream`、`download`、`getCoverArt`），`forward` 里的请求头原样带上（`Range` 这类）。
    /// 上游回的是 JSON 时按失败信封读。
    ///
    /// # Errors
    ///
    /// 同 [`Upstream::call`]；该回字节却回了成功的 JSON 时是 `protocol`。
    pub async fn fetch(
        &self,
        person: &str,
        method: &str,
        params: &Params,
        forward: &HeaderMap,
    ) -> Reply<reqwest::Response> {
        if let Attempt::Done(response) = self.fetch_once(person, method, params, forward).await? {
            return Ok(response);
        }
        self.open_web(person).await?;
        match self.fetch_once(person, method, params, forward).await? {
            Attempt::Done(response) => Ok(response),
            Attempt::NoAccount => Err(no_account(person)),
        }
    }

    async fn call_once(
        &self,
        person: &str,
        method: &str,
        params: &Params,
    ) -> Reply<Attempt<Value>> {
        let response = self.send(person, method, params, &HeaderMap::new()).await?;
        let body: Value = response.json().await.map_err(|e| {
            MediaFailure::protocol(format!("Navidrome 的 {method} 回的不是 JSON：{e}"))
        })?;
        envelope(method, body)
    }

    async fn fetch_once(
        &self,
        person: &str,
        method: &str,
        params: &Params,
        forward: &HeaderMap,
    ) -> Reply<Attempt<reqwest::Response>> {
        let response = self.send(person, method, params, forward).await?;
        let is_json = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json"));
        if !is_json {
            return Ok(Attempt::Done(response));
        }
        let body: Value = response.json().await.map_err(|e| {
            MediaFailure::protocol(format!("Navidrome 的 {method} 回的 JSON 读不出来：{e}"))
        })?;
        match envelope(method, body)? {
            Attempt::NoAccount => Ok(Attempt::NoAccount),
            Attempt::Done(_) => Err(MediaFailure::protocol(format!(
                "Navidrome 的 {method} 该回字节，回的是 JSON"
            ))),
        }
    }

    async fn send(
        &self,
        person: &str,
        method: &str,
        params: &Params,
        forward: &HeaderMap,
    ) -> Reply<reqwest::Response> {
        let mut query: Vec<(&str, &str)> =
            vec![("v", API_VERSION), ("c", CLIENT_NAME), ("f", "json")];
        query.extend(params.iter().map(|(name, value)| (*name, value.as_str())));
        let mut request = self
            .http
            .get(format!("{}/rest/{method}", self.base))
            .query(&query)
            .header(USER_HEADER, person);
        for (name, value) in forward {
            request = request.header(name, value);
        }
        let response = request.send().await.map_err(|e| transport(&e))?;
        checked(response)
    }

    async fn open_web(&self, person: &str) -> Reply<()> {
        let response = self
            .http
            .get(format!("{}/app/", self.base))
            .header(USER_HEADER, person)
            .send()
            .await
            .map_err(|e| transport(&e))?;
        checked(response).map(|_| ())
    }
}

fn envelope(method: &str, mut body: Value) -> Reply<Attempt<Value>> {
    let Some(inner) = body.get_mut("subsonic-response").map(Value::take) else {
        return Err(MediaFailure::protocol(format!(
            "Navidrome 的 {method} 回包里没有 subsonic-response"
        )));
    };
    if inner.get("status").and_then(Value::as_str) == Some("ok") {
        return Ok(Attempt::Done(inner));
    }
    let code = inner.pointer("/error/code").and_then(Value::as_i64);
    let message = inner
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    match code {
        Some(WRONG_CREDENTIALS) => Ok(Attempt::NoAccount),
        Some(NOT_AUTHORIZED) => Err(MediaFailure::new(
            "permission_denied",
            format!("Navidrome 不让做 {method}：{message}"),
        )),
        Some(NOT_FOUND) => Err(MediaFailure::new(
            "media_gone",
            format!("Navidrome 里找不到：{message}"),
        )),
        _ => Err(MediaFailure::upstream(format!(
            "Navidrome 的 {method} 失败（{}）：{message}",
            code.map_or_else(|| "没有错误码".to_owned(), |c| c.to_string())
        ))),
    }
}

fn checked(response: reqwest::Response) -> Reply<reqwest::Response> {
    let status = response.status();
    if status == StatusCode::TOO_MANY_REQUESTS {
        let retry = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok());
        return Err(MediaFailure {
            retry_after_secs: retry,
            ..MediaFailure::new("rate_limited", "Navidrome 限流")
        });
    }
    if status.is_server_error() {
        return Err(MediaFailure::upstream(format!(
            "Navidrome 回了 HTTP {status}"
        )));
    }
    if status.is_client_error() && status != StatusCode::RANGE_NOT_SATISFIABLE {
        return Err(MediaFailure::protocol(format!(
            "Navidrome 回了 HTTP {status}"
        )));
    }
    Ok(response)
}

fn transport(error: &reqwest::Error) -> MediaFailure {
    if error.is_connect() {
        MediaFailure::new("plugin_unavailable", format!("连不上 Navidrome：{error}"))
    } else {
        MediaFailure::upstream(format!("和 Navidrome 的连接出错：{error}"))
    }
}

fn no_account(person: &str) -> MediaFailure {
    MediaFailure::new(
        "authentication_required",
        format!("Navidrome 没有认出 {person}：打开过网页仍回错误 40，检查代理白名单"),
    )
}
