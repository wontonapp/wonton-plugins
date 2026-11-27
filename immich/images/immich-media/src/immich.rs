//! Immich API 的最小客户端：不重试、不缓存，错误按状态码分类（`failure::from_immich`）。
//! 钉住的是 compose 里那一版 Immich 的 OpenAPI；字段对不上就报 `protocol`，不写两套形状。

use reqwest::{Method, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::failure::{self, Failure, Result};

const USER_AGENT: &str = concat!("wonton-immich-media/", env!("CARGO_PKG_VERSION"));

/// 请求用哪把凭据：按人的 API key，或登录换来的会话令牌（只在开户时用）。
#[derive(Clone, Copy)]
pub enum Auth<'a> {
    Key(&'a str),
    Session(&'a str),
    None,
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: String,
}

impl Client {
    pub fn new(base: String) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("HTTP 客户端构造失败");
        Self { http, base }
    }

    pub fn request(&self, method: Method, path: &str, auth: Auth<'_>) -> RequestBuilder {
        let builder = self
            .http
            .request(method, format!("{}/api{path}", self.base));
        match auth {
            Auth::Key(key) => builder.header("x-api-key", key),
            Auth::Session(token) => builder.bearer_auth(token),
            Auth::None => builder,
        }
    }

    /// 发出去，非 2xx 按状态码分类。
    pub async fn send(&self, builder: RequestBuilder, what: &str) -> Result<Response> {
        self.send_allowing(builder, what, &[]).await
    }

    /// 同 [`Self::send`]，`also` 里的状态码也照常回（取字节时的 304、416）。
    pub async fn send_allowing(
        &self,
        builder: RequestBuilder,
        what: &str,
        also: &[u16],
    ) -> Result<Response> {
        let response = builder
            .send()
            .await
            .map_err(|err| failure::unreachable(what, &err))?;
        let status = response.status();
        if status.is_success() || also.contains(&status.as_u16()) {
            return Ok(response);
        }
        let body = response.text().await.unwrap_or_default();
        Err(failure::from_immich(status, what, &body))
    }

    /// 发 JSON、收 JSON；回包是空体（204）时给 `Value::Null`。
    pub async fn json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        auth: Auth<'_>,
        body: Option<&Value>,
    ) -> Result<T> {
        let what = format!("{method} {path}");
        let mut builder = self.request(method, path, auth);
        if let Some(body) = body {
            builder = builder.json(body);
        }
        let response = self.send(builder, &what).await?;
        let bytes = response
            .bytes()
            .await
            .map_err(|err| failure::unreachable(&what, &err))?;
        let value: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .map_err(|err| Failure::protocol(format!("Immich {what} 回的不是 JSON：{err}")))?
        };
        serde_json::from_value(value)
            .map_err(|err| Failure::protocol(format!("Immich {what} 的回包对不上：{err}")))
    }

    pub async fn ping(&self) -> Result<()> {
        let pong: Value = self
            .json(Method::GET, "/server/ping", Auth::None, None)
            .await?;
        if pong["res"] == "pong" {
            Ok(())
        } else {
            Err(Failure::protocol(format!("Immich ping 回了 {pong}")))
        }
    }

    /// 密码登录换会话令牌。
    pub async fn login(&self, email: &str, password: &str) -> Result<String> {
        let body = serde_json::json!({"email": email, "password": password});
        let reply: Value = self
            .json(Method::POST, "/auth/login", Auth::None, Some(&body))
            .await?;
        reply["accessToken"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Failure::protocol("Immich 登录回包里没有 accessToken"))
    }

    /// 用完就退出，不在 Immich 里留一堆会话；退出失败只记日志，登录本身已经成了。
    pub async fn logout(&self, token: &str) {
        let builder = self.request(Method::POST, "/auth/logout", Auth::Session(token));
        if let Err(err) = self.send(builder, "POST /auth/logout").await {
            eprintln!("Immich 会话退出失败：{}", err.message);
        }
    }
}
