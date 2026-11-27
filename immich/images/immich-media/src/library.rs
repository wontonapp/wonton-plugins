//! 相册浏览（契约的 `library` 操作）：时间线、相册、搜索、人物、详情、取字节和本人的写操作，
//! 全部用这个人自己的 Immich key，看到的就是他在 Immich 里能看到的（自己的、partner 给他的、共享相册）。

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::accounts::{Account, matrix_id};
use crate::failure::{Failure, JsonBody, Result};
use crate::immich::Auth;
use crate::library_map::{self, Users};
use crate::{App, actions};

/// 一页最多几条（搜索、相册里的条目）。
const PAGE_SIZE: u64 = 200;

/// 透传给 Immich、再从 Immich 透传回来的字节头：断点续读和缓存要的这几样。
const PASS_REQUEST: &[&str] = &["range", "if-range", "if-none-match", "if-modified-since"];
const PASS_RESPONSE: &[&str] = &[
    "content-type",
    "content-length",
    "content-range",
    "accept-ranges",
    "etag",
    "last-modified",
    "cache-control",
];

impl App {
    /// 这个人在 Immich 里能看到的用户，翻成 Matrix ID。
    pub async fn users(&self, account: &Account) -> Result<Users> {
        let list: Vec<Value> = self
            .immich
            .json(Method::GET, "/users", Auth::Key(&account.api_key), None)
            .await
            .map_err(|err| self.on_immich_error(&account.user, err))?;
        let ids = list
            .iter()
            .filter_map(|u| {
                let id = u["id"].as_str()?.to_owned();
                let email = u["email"].as_str()?;
                let user = self.accounts.user_of_email(email)?;
                Some((id, matrix_id(&user, &account.server)))
            })
            .collect();
        let household = self
            .household
            .as_deref()
            .map(|user| matrix_id(user, &account.server));
        Ok(Users { ids, household })
    }

    pub async fn call<T: serde::de::DeserializeOwned>(
        &self,
        account: &Account,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<T> {
        self.immich
            .json(method, path, Auth::Key(&account.api_key), body)
            .await
            .map_err(|err| self.on_immich_error(&account.user, err))
    }
}

/// 接哪些写操作：Immich 七样都接。
pub async fn features(State(app): State<App>, headers: HeaderMap) -> Result<Json<Value>> {
    app.person(&headers).await?;
    Ok(Json(json!({"actions": [
        "favorite",
        "create_album",
        "add_to_album",
        "remove_from_album",
        "share_album",
        "trash",
        "restore",
    ]})))
}

pub async fn timeline(State(app): State<App>, headers: HeaderMap) -> Result<Json<Value>> {
    let account = app.person(&headers).await?;
    let path = "/timeline/buckets?visibility=timeline&withPartners=true&order=desc";
    let buckets: Vec<Value> = app.call(&account, Method::GET, path, None).await?;
    let buckets: Vec<Value> = buckets
        .iter()
        .map(|b| {
            let month = b["timeBucket"]
                .as_str()
                .and_then(|t| t.get(..7))
                .ok_or_else(|| Failure::protocol("Immich 的时间桶没有 timeBucket"))?;
            Ok(json!({"month": month, "count": b["count"].as_u64().unwrap_or(0)}))
        })
        .collect::<Result<_>>()?;
    Ok(Json(json!({"buckets": buckets})))
}

/// 拼进 Immich 路径的 id 只能是 UUID：路径段解码后可能带 `/`、`?`，不校验就能拼出别的接口。
pub fn uuid(id: &str) -> Result<&str> {
    let ok = id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        });
    if ok {
        Ok(id)
    } else {
        Err(Failure::gone(format!("没有这一条：{id}")))
    }
}

fn valid_month(month: &str) -> bool {
    let bytes = month.as_bytes();
    bytes.len() == 7
        && bytes[4] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || b.is_ascii_digit())
}

pub async fn bucket(
    State(app): State<App>,
    headers: HeaderMap,
    Path(month): Path<String>,
) -> Result<Json<Value>> {
    let account = app.person(&headers).await?;
    if !valid_month(&month) {
        return Err(Failure::protocol(format!("月份要写成 YYYY-MM：{month}")));
    }
    let path = format!(
        "/timeline/bucket?timeBucket={month}-01&visibility=timeline&withPartners=true&order=desc"
    );
    let bucket: Value = app.call(&account, Method::GET, &path, None).await?;
    let users = app.users(&account).await?;
    let assets = library_map::bucket_assets(&bucket, &account.user_id, &users)?;
    Ok(Json(json!({"assets": assets})))
}

pub async fn albums(State(app): State<App>, headers: HeaderMap) -> Result<Json<Value>> {
    let account = app.person(&headers).await?;
    let list: Vec<Value> = app.call(&account, Method::GET, "/albums", None).await?;
    let users = app.users(&account).await?;
    let albums: Vec<Value> = list
        .iter()
        .map(|a| library_map::album(a, &account.user_id, &users))
        .collect::<Result<_>>()?;
    Ok(Json(json!({"albums": albums})))
}

#[derive(Deserialize)]
pub struct PageQuery {
    cursor: Option<String>,
}

fn page_number(cursor: Option<&str>) -> Result<u64> {
    match cursor {
        None => Ok(1),
        Some(raw) => raw
            .parse()
            .ok()
            .filter(|page| *page >= 1)
            .ok_or_else(|| Failure::protocol(format!("游标不对：{raw}"))),
    }
}

/// Immich 的搜索回包（`SearchResponseDto.assets`）翻成一页。
fn search_page(reply: &Value, account: &Account, users: &Users) -> Result<Value> {
    let found = &reply["assets"];
    let items = found["items"]
        .as_array()
        .ok_or_else(|| Failure::protocol("Immich 搜索回包没有 assets.items"))?;
    let assets: Vec<_> = items
        .iter()
        .map(|item| library_map::asset(item, &account.user_id, users))
        .collect::<Result<_>>()?;
    let mut page = json!({"assets": assets});
    if let Some(next) = found["nextPage"].as_str() {
        page["cursor"] = next.into();
    }
    Ok(page)
}

pub async fn album(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Value>> {
    let account = app.person(&headers).await?;
    let id = uuid(&id)?;
    let page = page_number(query.cursor.as_deref())?;
    let detail: Value = app
        .call(&account, Method::GET, &format!("/albums/{id}"), None)
        .await?;
    let body = json!({"albumIds": [id], "page": page, "size": PAGE_SIZE, "order": "desc"});
    let reply: Value = app
        .call(&account, Method::POST, "/search/metadata", Some(&body))
        .await?;
    let users = app.users(&account).await?;
    let album = library_map::album(&detail, &account.user_id, &users)?;
    let mut out = search_page(&reply, &account, &users)?;
    out["album"] = album;
    Ok(Json(out))
}

#[derive(Deserialize, Default)]
pub struct SearchQuery {
    q: Option<String>,
    people: Option<String>,
    place: Option<String>,
    from: Option<u64>,
    to: Option<u64>,
    kind: Option<String>,
    favorite: Option<bool>,
    cursor: Option<String>,
}

fn iso(ms: u64) -> Result<String> {
    chrono::DateTime::from_timestamp_millis(ms as i64)
        .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .ok_or_else(|| Failure::protocol(format!("时间超出范围：{ms}")))
}

/// 搜索条件翻成 Immich 的 `SmartSearchDto`/`MetadataSearchDto`（两者的筛选字段同名）。
fn search_body(query: &SearchQuery, place_key: &str) -> Result<Value> {
    let mut body = json!({"page": page_number(query.cursor.as_deref())?, "size": PAGE_SIZE});
    if let Some(q) = query.q.as_deref().filter(|q| !q.trim().is_empty()) {
        body["query"] = q.into();
    }
    if let Some(people) = &query.people {
        let ids: Vec<&str> = people.split(',').filter(|id| !id.is_empty()).collect();
        body["personIds"] = ids.into();
    }
    if let Some(place) = &query.place {
        body[place_key] = place.as_str().into();
    }
    if let Some(from) = query.from {
        body["takenAfter"] = iso(from)?.into();
    }
    if let Some(to) = query.to {
        body["takenBefore"] = iso(to)?.into();
    }
    match query.kind.as_deref() {
        None => {}
        Some("photo") => body["type"] = "IMAGE".into(),
        Some("video") => body["type"] = "VIDEO".into(),
        Some(other) => return Err(Failure::protocol(format!("不认识的种类：{other}"))),
    }
    if let Some(favorite) = query.favorite {
        body["isFavorite"] = favorite.into();
    }
    let any = [
        "query",
        "personIds",
        place_key,
        "takenAfter",
        "takenBefore",
        "type",
        "isFavorite",
    ]
    .iter()
    .any(|key| !body[*key].is_null());
    if !any {
        return Err(Failure::protocol("搜索至少要一个条件"));
    }
    Ok(body)
}

pub async fn search(
    State(app): State<App>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Value>> {
    let account = app.person(&headers).await?;
    let path = if query.q.as_deref().is_some_and(|q| !q.trim().is_empty()) {
        "/search/smart"
    } else {
        "/search/metadata"
    };
    let users = app.users(&account).await?;
    // 地名可能是城市也可能是国家（“青岛”“日本”），Immich 分两个字段：先按城市，一条没有再按国家。
    let mut reply: Value = app
        .call(
            &account,
            Method::POST,
            path,
            Some(&search_body(&query, "city")?),
        )
        .await?;
    let empty = reply["assets"]["items"]
        .as_array()
        .is_some_and(Vec::is_empty);
    if query.place.is_some() && empty {
        let body = search_body(&query, "country")?;
        reply = app.call(&account, Method::POST, path, Some(&body)).await?;
    }
    Ok(Json(search_page(&reply, &account, &users)?))
}

pub async fn people(State(app): State<App>, headers: HeaderMap) -> Result<Json<Value>> {
    let account = app.person(&headers).await?;
    let reply: Value = app
        .call(
            &account,
            Method::GET,
            "/people?withHidden=false&size=500",
            None,
        )
        .await?;
    let people: Vec<Value> = reply["people"]
        .as_array()
        .ok_or_else(|| Failure::protocol("Immich /people 没有 people"))?
        .iter()
        .filter(|p| p["name"].as_str().is_some_and(|n| !n.is_empty()))
        .map(|p| json!({"id": p["id"], "name": p["name"]}))
        .collect();
    Ok(Json(json!({"people": people})))
}

pub async fn item(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let account = app.person(&headers).await?;
    let id = uuid(&id)?;
    let value: Value = app
        .call(&account, Method::GET, &format!("/assets/{id}"), None)
        .await?;
    let users = app.users(&account).await?;
    Ok(Json(library_map::detail(&value, &account.user_id, &users)?))
}

#[derive(Deserialize)]
pub struct ContentQuery {
    rendition: String,
}

/// 档位 → Immich 的字节路径。实况的动态部分要先查出它那段视频的 id。
async fn content_path(app: &App, account: &Account, id: &str, rendition: &str) -> Result<String> {
    let id = uuid(id)?;
    Ok(match rendition {
        "thumbnail" => format!("/assets/{id}/thumbnail?size=thumbnail"),
        "preview" => format!("/assets/{id}/thumbnail?size=preview"),
        "original" => format!("/assets/{id}/original"),
        "playback" => format!("/assets/{id}/video/playback"),
        "live" => {
            let value: Value = app
                .call(account, Method::GET, &format!("/assets/{id}"), None)
                .await?;
            let video = value["livePhotoVideoId"]
                .as_str()
                .ok_or_else(|| Failure::gone("这一条不是实况照片"))
                .and_then(uuid)?;
            format!("/assets/{video}/video/playback")
        }
        other => return Err(Failure::protocol(format!("不认识的档位：{other}"))),
    })
}

async fn stream(app: &App, account: &Account, path: &str, headers: &HeaderMap) -> Result<Response> {
    let mut builder = app
        .immich
        .request(Method::GET, path, Auth::Key(&account.api_key));
    for name in PASS_REQUEST {
        if let Some(value) = headers.get(*name) {
            builder = builder.header(*name, value);
        }
    }
    // 条件请求命中（304）和 `Range` 越界（416）照原样回，客户端核的缓存和续传靠它们。
    let upstream = app
        .immich
        .send_allowing(builder, &format!("GET {path}"), &[304, 416])
        .await
        .map_err(|err| app.on_immich_error(&account.user, err))?;
    let mut response = Response::builder().status(upstream.status().as_u16());
    for name in PASS_RESPONSE {
        if let Some(value) = upstream.headers().get(*name) {
            response = response.header(*name, value);
        }
    }
    response
        .body(Body::from_stream(upstream.bytes_stream()))
        .map_err(|err| Failure::upstream(format!("拼回包失败：{err}")))
}

pub async fn content(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<ContentQuery>,
) -> Result<Response> {
    let account = app.person(&headers).await?;
    let path = content_path(&app, &account, &id, &query.rendition).await?;
    stream(&app, &account, &path, &headers).await
}

pub async fn person_thumbnail(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response> {
    let account = app.person(&headers).await?;
    let id = uuid(&id)?;
    stream(&app, &account, &format!("/people/{id}/thumbnail"), &headers).await
}

pub async fn action(
    State(app): State<App>,
    headers: HeaderMap,
    JsonBody(action): JsonBody<actions::LibraryAction>,
) -> Result<(StatusCode, Json<Value>)> {
    let account = app.person(&headers).await?;
    let result = actions::run(&app, &account, action).await?;
    Ok((StatusCode::OK, Json(result)))
}
