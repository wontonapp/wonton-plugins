//! 媒体源契约的错误信封 `{code, message, retry_after_secs?}`。码取规范“媒体源：错误信封”表的第一列，
//! 核心按码决定退避，所以这里只做归类，不重试。

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    Protocol,
    AuthenticationRequired,
    PermissionDenied,
    UnsupportedByDevice,
    RateLimited,
    MediaGone,
    PluginUnavailable,
    UpstreamError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Failure {
    pub code: Code,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u32>,
}

impl Failure {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retry_after_secs: None,
        }
    }

    /// 请求本身不合法（缺头、坏参数、会话不属于这个人）。规范没有“输入非法”这一类，按对不上算。
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(Code::Protocol, message)
    }

    /// 按 Jellyfin 回的状态归类。`what` 说清是哪一步，进 `message` 给人看。
    pub fn from_upstream(status: u16, what: &str, retry_after: Option<u32>) -> Self {
        let (code, text) = match status {
            401 => (Code::AuthenticationRequired, "Jellyfin 不认这把凭据"),
            403 => (Code::PermissionDenied, "Jellyfin 不让这个人访问"),
            404 => (Code::MediaGone, "Jellyfin 里没有这一项"),
            429 => (Code::RateLimited, "Jellyfin 限流"),
            500..=599 => (Code::UpstreamError, "Jellyfin 出错"),
            _ => (Code::Protocol, "Jellyfin 回了意料之外的状态"),
        };
        Self {
            code,
            message: format!("{what}：{text}（HTTP {status}）"),
            retry_after_secs: if code == Code::RateLimited {
                retry_after
            } else {
                None
            },
        }
    }

    /// 连不上 Jellyfin：它和适配器同属一个插件，按插件没起来算。
    pub fn unreachable(what: &str, err: &reqwest::Error) -> Self {
        Self::new(
            Code::PluginUnavailable,
            format!("{what}：连不上 Jellyfin（{err}）"),
        )
    }

    pub fn status(&self) -> StatusCode {
        match self.code {
            Code::Protocol => StatusCode::BAD_REQUEST,
            Code::AuthenticationRequired => StatusCode::UNAUTHORIZED,
            Code::PermissionDenied => StatusCode::FORBIDDEN,
            Code::UnsupportedByDevice => StatusCode::UNPROCESSABLE_ENTITY,
            Code::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Code::MediaGone => StatusCode::NOT_FOUND,
            Code::PluginUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Code::UpstreamError => StatusCode::BAD_GATEWAY,
        }
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (self.status(), Json(self)).into_response()
    }
}

pub type Reply<T> = Result<T, Failure>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_statuses_map_to_contract_codes() {
        assert_eq!(
            Failure::from_upstream(401, "x", None).code,
            Code::AuthenticationRequired
        );
        assert_eq!(
            Failure::from_upstream(403, "x", None).code,
            Code::PermissionDenied
        );
        assert_eq!(Failure::from_upstream(404, "x", None).code, Code::MediaGone);
        assert_eq!(
            Failure::from_upstream(503, "x", None).code,
            Code::UpstreamError
        );
        assert_eq!(Failure::from_upstream(418, "x", None).code, Code::Protocol);
    }

    #[test]
    fn retry_after_only_rides_on_rate_limits() {
        assert_eq!(
            Failure::from_upstream(429, "x", Some(30)).retry_after_secs,
            Some(30)
        );
        assert_eq!(
            Failure::from_upstream(503, "x", Some(30)).retry_after_secs,
            None
        );
    }

    #[test]
    fn envelope_shape_matches_spec() {
        let body = serde_json::to_value(Failure::new(Code::MediaGone, "没了")).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"code": "media_gone", "message": "没了"})
        );
    }
}
