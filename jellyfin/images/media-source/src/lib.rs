//! Jellyfin 的媒体源适配器：把规范“媒体源：路径”那张表翻成 Jellyfin 12.4 的 HTTP 接口。只做字段映射、
//! 错误归类和按人注入凭据；重试、断点、队列、会话状态都在核心，适配器重启不丢东西。

pub mod accounts;
pub mod contract;
pub mod failure;
pub mod items;
pub mod jellyfin;
pub mod playback;
pub mod profile;
pub mod proxy;
pub mod store;

use std::future::Future;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;

use accounts::{Account, Accounts};
use contract::{Capabilities, PlaybackRequest, Progress, Revoke, Stopped};
use failure::{Code, Failure, Reply};
use items::ListQuery;
use jellyfin::{Jellyfin, encode};
use playback::Session;
use store::Credentials;

/// 本适配器真做得到的：Jellyfin 能列片、出播放计划、给下载；不能收上传。音乐交给音乐的提供方，这里只报视频。
pub const CAPABILITIES: Capabilities = Capabilities {
    operations: &["list", "playback", "download"],
    kinds: &["video"],
};

pub const USER_HEADER: &str = "x-wonton-user";

pub struct App {
    pub jellyfin: Jellyfin,
    pub accounts: Accounts,
    pub credentials: Credentials,
}

type Shared = State<Arc<App>>;

pub fn router(app: Arc<App>) -> Router {
    let api = Router::new()
        .route(
            "/_wonton/capabilities",
            get(|| async { Json(CAPABILITIES) }),
        )
        .route("/items", get(list))
        .route("/items/{id}/segments", get(segments))
        .route("/items/{id}/download", get(download))
        .route("/items/{id}/artwork", get(artwork))
        .route("/playback", post(start))
        .route("/playback/{session}/progress", post(progress))
        .route("/playback/{session}/queue", put(queue))
        .route("/playback/{session}/stopped", post(stopped))
        .route("/playback/{session}/stream", get(stream))
        .route("/playback/{session}/hls/{*rest}", get(hls))
        .route("/revoke", post(revoke))
        .layer(middleware::from_fn_with_state(app.clone(), authorize));
    Router::new()
        .route("/healthz", get(healthz))
        .merge(api)
        .with_state(app)
}

/// 只认核心签给本提供方的凭据（`credentials issue`）；一把都没签时谁都不放行。
async fn authorize(State(app): Shared, req: Request, next: Next) -> Response {
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if presented.is_empty() || !app.credentials.accepts(presented) {
        return Failure::new(
            Code::AuthenticationRequired,
            "没有带核心签发的凭据，或凭据已收回",
        )
        .into_response();
    }
    next.run(req).await
}

/// 健康检查查的是真能用：Jellyfin 在、管理员 key 有效。
async fn healthz(State(app): Shared) -> Response {
    match app
        .jellyfin
        .get::<serde_json::Value>("/System/Info", app.accounts.admin(), "查 Jellyfin 状态")
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(f) => f.into_response(),
    }
}

fn person(headers: &HeaderMap) -> Reply<String> {
    headers
        .get(USER_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Failure::bad_request("缺 X-Wonton-User：认不出是谁，不拿别人兜底"))
}

/// 以这个人的身份做一件事。本人令牌被 Jellyfin 拒了（管理员在 Jellyfin 里删了设备）就重拿一次再试。
async fn as_person<T, F, Fut>(app: &App, headers: &HeaderMap, op: F) -> Reply<T>
where
    F: Fn(Account) -> Fut,
    Fut: Future<Output = Reply<T>>,
{
    let person = person(headers)?;
    let account = app.accounts.get(&person).await?;
    match op(account).await {
        Err(f) if f.code == Code::AuthenticationRequired => {
            app.accounts.forget(&person).await?;
            op(app.accounts.get(&person).await?).await
        }
        other => other,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

async fn list(
    State(app): Shared,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Reply<Response> {
    let page = as_person(&app, &headers, |who| {
        let app = app.clone();
        let query = &query;
        async move { items::list(&app.jellyfin, &who, query).await }
    })
    .await?;
    Ok(Json(page).into_response())
}

async fn segments(
    State(app): Shared,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Reply<Response> {
    let found = as_person(&app, &headers, |who| {
        let app = app.clone();
        let id = id.clone();
        async move { items::segments(&app.jellyfin, &who, &id).await }
    })
    .await?;
    Ok(Json(serde_json::json!({ "segments": found })).into_response())
}

async fn download(
    State(app): Shared,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Reply<Response> {
    let path = format!("/Items/{}/Download", encode(&id));
    as_person(&app, &headers, |who| {
        let (app, path, headers) = (app.clone(), path.clone(), headers.clone());
        async move { proxy::forward(&app.jellyfin, &who, &path, &headers, "下载").await }
    })
    .await
}

#[derive(Deserialize)]
struct ArtworkQuery {
    #[serde(rename = "type")]
    kind: Option<String>,
    width: Option<u32>,
}

async fn artwork(
    State(app): Shared,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<ArtworkQuery>,
) -> Reply<Response> {
    let kind = match q.kind.as_deref().unwrap_or("primary") {
        "primary" => "Primary",
        "backdrop" => "Backdrop",
        "thumb" => "Thumb",
        "logo" => "Logo",
        other => return Err(Failure::bad_request(format!("不认识的图片种类：{other}"))),
    };
    let width = q.width.unwrap_or(600).clamp(64, 3840);
    let path = format!(
        "/Items/{}/Images/{kind}?maxWidth={width}&quality=90",
        encode(&id)
    );
    as_person(&app, &headers, |who| {
        let (app, path, headers) = (app.clone(), path.clone(), headers.clone());
        async move { proxy::forward(&app.jellyfin, &who, &path, &headers, "取海报").await }
    })
    .await
}

async fn start(
    State(app): Shared,
    headers: HeaderMap,
    Json(req): Json<PlaybackRequest>,
) -> Reply<Response> {
    let plan = as_person(&app, &headers, |who| {
        let (app, req) = (app.clone(), &req);
        async move { playback::plan(&app.jellyfin, &who, req, now_ms()).await }
    })
    .await?;
    Ok(Json(plan).into_response())
}

/// 先核对会话属于这个人，再以他的身份做。
async fn with_session<T, F, Fut>(app: &Arc<App>, headers: &HeaderMap, raw: &str, op: F) -> Reply<T>
where
    F: Fn(Account, Session) -> Fut,
    Fut: Future<Output = Reply<T>>,
{
    let session = Session::decode(raw, &person(headers)?)?;
    as_person(app, headers, |who| op(who, session.clone())).await
}

async fn progress(
    State(app): Shared,
    headers: HeaderMap,
    Path(raw): Path<String>,
    Json(report): Json<Progress>,
) -> Reply<StatusCode> {
    with_session(&app, &headers, &raw, |who, s| {
        let (app, report) = (app.clone(), &report);
        async move { playback::progress(&app.jellyfin, &who, &s, report).await }
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Jellyfin 没有服务器端的播放队列；队列由核心持有，这里核对会话后照规范回 204。
async fn queue(headers: HeaderMap, Path(raw): Path<String>) -> Reply<StatusCode> {
    Session::decode(&raw, &person(&headers)?)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stopped(
    State(app): Shared,
    headers: HeaderMap,
    Path(raw): Path<String>,
    Json(report): Json<Stopped>,
) -> Reply<StatusCode> {
    with_session(&app, &headers, &raw, |who, s| {
        let (app, report) = (app.clone(), &report);
        async move { playback::stopped(&app.jellyfin, &who, &s, report).await }
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stream(
    State(app): Shared,
    headers: HeaderMap,
    Path(raw): Path<String>,
) -> Reply<Response> {
    with_session(&app, &headers, &raw, |who, s| {
        let (app, headers) = (app.clone(), headers.clone());
        async move {
            let path = format!(
                "/Videos/{}/stream?static=true&mediaSourceId={}&playSessionId={}",
                encode(&s.i),
                encode(&s.m),
                encode(&s.p)
            );
            proxy::forward(&app.jellyfin, &who, &path, &headers, "取视频流").await
        }
    })
    .await
}

/// HLS 的主列表、子列表和分片都走这里：播放列表里的地址是相对的，播放器按本路径解析回来，
/// 所以把 `/videos/<条目>/` 之后的部分原样接上即可；播放列表只剥掉 Jellyfin 写进去的令牌。
async fn hls(
    State(app): Shared,
    headers: HeaderMap,
    Path((raw, rest)): Path<(String, String)>,
    uri: axum::http::Uri,
) -> Reply<Response> {
    if rest.split('/').any(|seg| seg == ".." || seg.is_empty()) {
        return Err(Failure::bad_request("HLS 路径不合法"));
    }
    let playlist = rest.ends_with(".m3u8");
    let tail = match uri.query() {
        Some(q) => playback::strip_credentials(&format!("{rest}?{q}")),
        None => rest,
    };
    with_session(&app, &headers, &raw, |who, s| {
        let (app, headers, tail) = (app.clone(), headers.clone(), tail.clone());
        async move {
            let path = format!("/videos/{}/{tail}", encode(&s.i));
            if playlist {
                proxy::forward_playlist(&app.jellyfin, &who, &path, &headers, "取播放列表").await
            } else {
                proxy::forward(&app.jellyfin, &who, &path, &headers, "取转码流").await
            }
        }
    })
    .await
}

async fn revoke(State(app): Shared, Json(body): Json<Revoke>) -> Reply<StatusCode> {
    app.accounts.revoke(&body.name).await?;
    Ok(StatusCode::NO_CONTENT)
}
