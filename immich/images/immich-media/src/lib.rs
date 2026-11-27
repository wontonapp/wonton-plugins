//! Immich 的媒体源适配器：把核心的 `media_source` 契约翻成 Immich 自己的接口，
//! 只做字段映射、错误码归一和按人取凭据；不重试、不排队，任务和状态在核心。
//!
//! 每个请求带核心签的能力凭据（`Authorization: Bearer`）和 `X-Wonton-User`（替谁办），适配器用这个人
//! 自己的 Immich key 去调，看到的、改得了的就是他在 Immich 里的权限。

pub mod accounts;
pub mod actions;
pub mod auth;
pub mod failure;
pub mod immich;
pub mod library;
pub mod library_map;
pub mod store;
pub mod uploads;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use accounts::{Account, Accounts};
use failure::{Code, Failure, Result};

pub const DEFAULT_PORT: u16 = 8090;
const USER_HEADER: &str = "x-wonton-user";

#[derive(Clone)]
pub struct App {
    pub state: PathBuf,
    pub immich: immich::Client,
    pub accounts: Arc<Accounts>,
    /// 家庭相册的库主（本机用户名）；没设就不单列家庭相册
    pub household: Option<String>,
}

impl App {
    /// 核过能力凭据，取 `X-Wonton-User` 这个人的 Immich 账号（第一次就开户）。
    pub async fn person(&self, headers: &HeaderMap) -> Result<Account> {
        let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
        auth::check(&self.state, header("authorization"))?;
        let user = accounts::username(header(USER_HEADER))?;
        self.accounts.get(&user).await
    }

    /// Immich 不认这个人的 key 了（他在 Immich 里删了它）：丢掉记录，下一次请求重新开户。
    pub fn on_immich_error(&self, user: &str, err: Failure) -> Failure {
        if err.code == Code::AuthenticationRequired {
            self.accounts.forget(user);
        }
        err
    }
}

async fn capabilities() -> Json<Value> {
    Json(json!({"operations": ["upload", "library"], "kinds": ["photo", "video"]}))
}

async fn healthz(State(app): State<App>) -> (StatusCode, &'static str) {
    match tokio::time::timeout(Duration::from_secs(8), app.immich.ping()).await {
        Ok(Ok(())) => (StatusCode::OK, "ok"),
        Ok(Err(err)) => {
            eprintln!("healthz：{}", err.message);
            (StatusCode::SERVICE_UNAVAILABLE, "not ready")
        }
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "timeout"),
    }
}

pub fn router(app: App) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/_wonton/capabilities", get(capabilities))
        .route("/uploads", post(uploads::start))
        .route("/uploads/check", post(uploads::check))
        .route(
            "/uploads/{id}",
            axum::routing::head(uploads::head)
                .patch(uploads::patch)
                .delete(uploads::delete),
        )
        .route("/uploads/{id}/finish", post(uploads::finish))
        .route("/library", get(library::features))
        .route("/library/timeline", get(library::timeline))
        .route("/library/timeline/{month}", get(library::bucket))
        .route("/library/albums", get(library::albums))
        .route("/library/albums/{id}", get(library::album))
        .route("/library/search", get(library::search))
        .route("/library/people", get(library::people))
        .route(
            "/library/people/{id}/thumbnail",
            get(library::person_thumbnail),
        )
        .route("/library/items/{id}", get(library::item))
        .route("/library/items/{id}/content", get(library::content))
        .route("/library/actions", post(library::action))
        // 续写的分片和收尾的整个文件都流式落盘，不按体大小拦；JSON 体由各自的形状限制。
        .layer(DefaultBodyLimit::disable())
        .with_state(app)
}

/// 暂存超过一周仍未收尾的上传：核心那边的任务早已结束或换了新的上传号，留着只占盘。
pub fn sweep_stale_uploads(app: &App, older_than: Duration) {
    let root = app.state.join("uploads");
    let Ok(users) = std::fs::read_dir(&root) else {
        return;
    };
    for user in users.flatten() {
        let Ok(uploads) = std::fs::read_dir(user.path()) else {
            continue;
        };
        for upload in uploads.flatten() {
            let stale = upload
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|at| at.elapsed().ok())
                .is_some_and(|age| age > older_than);
            if stale && let Err(err) = std::fs::remove_dir_all(upload.path()) {
                eprintln!("清旧暂存失败（{}）：{err}", upload.path().display());
            }
        }
    }
}
