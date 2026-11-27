//! 路由：插件规范“媒体源：路径”里这个适配器实现的那几条。除了能力探测，每个请求都要带核心签的凭据和
//! `X-Wonton-User`。

use std::sync::Arc;

use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;

use crate::failure::{MediaFailure, Reply};
use crate::keys::KeyStore;
use crate::upstream::Upstream;
use crate::wire::{
    Capabilities, ItemsPage, ItemsQuery, PlaybackPlan, PlaybackProgress, PlaybackRequest,
    PlaybackStopped, QueueSnapshot,
};
use crate::{bytes, items, playback};

pub const USER_HEADER: &str = "x-wonton-user";

pub const CAPABILITIES: Capabilities = Capabilities {
    operations: &["download", "playback", "list"],
    kinds: &["audio"],
};

#[derive(Clone)]
pub struct AppState {
    pub upstream: Upstream,
    pub keys: Arc<KeyStore>,
}

/// 请求是替谁做的。
struct Person(String);

pub fn router(state: AppState) -> Router {
    let guarded = Router::new()
        .route("/items", get(list))
        .route("/items/{id}/stream", get(stream))
        .route("/items/{id}/download", get(download))
        .route("/items/{id}/artwork", get(artwork))
        .route("/playback", post(plan))
        .route("/playback/{session}/progress", post(progress))
        .route("/playback/{session}/queue", put(queue))
        .route("/playback/{session}/stopped", post(stopped))
        .route("/revoke", post(revoke))
        .route_layer(middleware::from_fn_with_state(state.clone(), authorize));
    Router::new()
        // 健康检查也打它，不要凭据；只回静态的能力表，不碰任何人的数据。
        .route(
            "/_wonton/capabilities",
            get(|| async { Json(CAPABILITIES) }),
        )
        .merge(guarded)
        .with_state(state)
}

async fn authorize(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let presented = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if !presented.is_some_and(|key| state.keys.accepts(key)) {
        return MediaFailure::new("authentication_required", "没有核心签发的凭据").into_response();
    }
    let person = request
        .headers()
        .get(USER_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let Some(person) = person else {
        return MediaFailure::protocol("请求没带 X-Wonton-User").into_response();
    };
    request.extensions_mut().insert(Arc::new(person));
    next.run(request).await
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Person {
    type Rejection = MediaFailure;

    fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        // 只读中间件放进去的扩展，不用等。
        std::future::ready(
            parts
                .extensions
                .get::<Arc<String>>()
                .map(|person| Self(person.as_ref().clone()))
                .ok_or_else(|| MediaFailure::protocol("请求没带 X-Wonton-User")),
        )
    }
}

async fn list(
    State(state): State<AppState>,
    Person(person): Person,
    Query(query): Query<ItemsQuery>,
) -> Reply<Json<ItemsPage>> {
    items::list(&state.upstream, &person, &query)
        .await
        .map(Json)
}

#[derive(Deserialize)]
struct StreamQuery {
    format: Option<String>,
    max_bitrate: Option<u32>,
}

async fn stream(
    State(state): State<AppState>,
    Person(person): Person,
    Path(id): Path<String>,
    Query(query): Query<StreamQuery>,
    headers: HeaderMap,
) -> Reply<Response> {
    let format = query.format.as_deref();
    bytes::stream(
        &state.upstream,
        &person,
        &id,
        format,
        query.max_bitrate,
        &headers,
    )
    .await
}

async fn download(
    State(state): State<AppState>,
    Person(person): Person,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Reply<Response> {
    bytes::download(&state.upstream, &person, &id, &headers).await
}

#[derive(Deserialize)]
struct ArtworkQuery {
    size: Option<u32>,
}

async fn artwork(
    State(state): State<AppState>,
    Person(person): Person,
    Path(id): Path<String>,
    Query(query): Query<ArtworkQuery>,
) -> Reply<Response> {
    bytes::artwork(&state.upstream, &person, &id, query.size).await
}

async fn plan(
    State(state): State<AppState>,
    Person(person): Person,
    Json(request): Json<PlaybackRequest>,
) -> Reply<Json<PlaybackPlan>> {
    playback::plan(&state.upstream, &person, &request)
        .await
        .map(Json)
}

async fn progress(
    State(state): State<AppState>,
    Person(person): Person,
    Path(session): Path<String>,
    Json(_): Json<PlaybackProgress>,
) -> Reply<StatusCode> {
    playback::progress(&state.upstream, &person, &session).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn queue(
    State(state): State<AppState>,
    Person(person): Person,
    Path(_session): Path<String>,
    Json(queue): Json<QueueSnapshot>,
) -> Reply<StatusCode> {
    playback::save_queue(&state.upstream, &person, &queue).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stopped(
    State(state): State<AppState>,
    Person(person): Person,
    Path(session): Path<String>,
    Json(stop): Json<PlaybackStopped>,
) -> Reply<StatusCode> {
    playback::stopped(&state.upstream, &person, &session, &stop).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 上游不存这个人的口令（身份靠代理头），也没有要停的上游会话，收回时无事可做。
async fn revoke() -> StatusCode {
    StatusCode::NO_CONTENT
}
