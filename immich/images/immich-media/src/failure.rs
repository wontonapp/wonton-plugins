//! 契约的失败信封：`{code, message, retry_after_secs?}`，码就是 ADR-0113 失败类别的名字，文案由壳按类别出，
//! `message` 只给排查看。HTTP 状态码跟着类别走，核心按码不按状态码判。

use axum::Json;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    /// 核心带的能力凭据不对，或这个人的 Immich 密钥失效
    AuthenticationRequired,
    /// Immich 拒了这个人的这项操作
    PermissionDenied,
    /// 请求不合契约，或 Immich 回了看不懂的东西
    Protocol,
    /// 条目或上传不存在
    MediaGone,
    UpstreamError,
    /// Immich 没起来
    PluginUnavailable,
    ServerStorageFull,
    RateLimited,
}

#[derive(Debug, Clone, Serialize)]
pub struct Failure {
    pub code: Code,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u32>,
}

pub type Result<T> = std::result::Result<T, Failure>;

impl Failure {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retry_after_secs: None,
        }
    }

    pub fn protocol(message: impl Into<String>) -> Self {
        Self::new(Code::Protocol, message)
    }

    pub fn gone(message: impl Into<String>) -> Self {
        Self::new(Code::MediaGone, message)
    }

    pub fn upstream(message: impl Into<String>) -> Self {
        Self::new(Code::UpstreamError, message)
    }

    /// 本地状态卷读写失败：写满了是服务器空间不足，其余是上游（适配器自己）出错。
    pub fn io(what: &str, err: &std::io::Error) -> Self {
        if err.raw_os_error() == Some(28) {
            return Self::new(Code::ServerStorageFull, format!("{what}：磁盘满了"));
        }
        Self::upstream(format!("{what}：{err}"))
    }

    fn status(&self) -> StatusCode {
        match self.code {
            Code::AuthenticationRequired => StatusCode::UNAUTHORIZED,
            Code::PermissionDenied => StatusCode::FORBIDDEN,
            Code::Protocol => StatusCode::BAD_REQUEST,
            Code::MediaGone => StatusCode::NOT_FOUND,
            Code::UpstreamError => StatusCode::BAD_GATEWAY,
            Code::PluginUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Code::ServerStorageFull => StatusCode::INSUFFICIENT_STORAGE,
            Code::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        }
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (self.status(), Json(self)).into_response()
    }
}

/// 按 Immich 的状态码分类，原话进 `message`。
pub fn from_immich(status: reqwest::StatusCode, what: &str, body: &str) -> Failure {
    let message = format!("Immich {what} 返回 {status}：{}", truncate(body, 200));
    match status.as_u16() {
        401 => Failure::new(Code::AuthenticationRequired, message),
        403 => Failure::new(Code::PermissionDenied, message),
        404 => Failure::gone(message),
        400 | 422 => Failure::protocol(message),
        413 | 507 => Failure::new(Code::ServerStorageFull, message),
        429 => Failure::new(Code::RateLimited, message),
        502..=504 => Failure::new(Code::PluginUnavailable, message),
        _ => Failure::upstream(message),
    }
}

/// 连不上 Immich：服务没起来或网络断了。
pub fn unreachable(what: &str, err: &reqwest::Error) -> Failure {
    let mut message = format!("请求 Immich {what} 失败：{err}");
    let mut source = std::error::Error::source(err);
    while let Some(inner) = source {
        message.push_str(&format!("：{inner}"));
        source = inner.source();
    }
    Failure::new(Code::PluginUnavailable, message)
}

pub fn truncate(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

/// JSON 体；形状不对回契约的 `protocol` 信封，不用 axum 自带的纯文本 422。
pub struct JsonBody<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for JsonBody<T> {
    type Rejection = Failure;

    async fn from_request(req: Request, state: &S) -> std::result::Result<Self, Failure> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(Failure::protocol(format!(
                "请求体不合契约：{}",
                rejection.body_text()
            ))),
        }
    }
}
