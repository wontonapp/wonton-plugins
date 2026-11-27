//! 规范“媒体源：错误信封”：失败回 `{code, message, retry_after_secs?}`，HTTP 状态只是辅助。

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MediaFailure {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u32>,
}

pub type Reply<T> = Result<T, MediaFailure>;

impl MediaFailure {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retry_after_secs: None,
        }
    }

    pub fn protocol(message: impl Into<String>) -> Self {
        Self::new("protocol", message)
    }

    pub fn upstream(message: impl Into<String>) -> Self {
        Self::new("upstream_error", message)
    }

    fn status(&self) -> StatusCode {
        match self.code {
            "authentication_required" => StatusCode::UNAUTHORIZED,
            "permission_denied" => StatusCode::FORBIDDEN,
            "media_gone" => StatusCode::NOT_FOUND,
            "unsupported_by_device" => StatusCode::UNPROCESSABLE_ENTITY,
            "rate_limited" => StatusCode::TOO_MANY_REQUESTS,
            "protocol" => StatusCode::BAD_REQUEST,
            "plugin_unavailable" => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::BAD_GATEWAY,
        }
    }
}

impl IntoResponse for MediaFailure {
    fn into_response(self) -> Response {
        (self.status(), Json(self)).into_response()
    }
}
