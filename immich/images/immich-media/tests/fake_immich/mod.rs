//! 测试用的假 Immich：只实现适配器用到的那几条接口，形状照钉住版本的 OpenAPI。
//! 状态都在内存里，测试直接读它断言“落到了谁的库里”。每个测试文件各编一份，用不到的 helper 不少。
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::{Multipart, OriginalUri, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::{Value, json};
#[derive(Clone, Debug)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password: String,
}

#[derive(Clone, Debug)]
pub struct Asset {
    pub id: String,
    pub owner: String,
    pub checksum: String,
    pub live: Option<String>,
    pub name: String,
    pub size: usize,
    pub created: String,
    pub favorite: bool,
    pub trashed: bool,
    pub file_missing: bool,
    pub city: Option<String>,
    pub country: Option<String>,
    pub person: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Person {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub file_missing: bool,
}

#[derive(Default)]
pub struct World {
    pub users: Vec<User>,
    pub keys: HashMap<String, String>,
    pub sessions: HashMap<String, String>,
    pub assets: Vec<Asset>,
    pub albums: Vec<(String, String, Vec<String>, Vec<String>)>,
    pub people: Vec<Person>,
    pub partners: Vec<(String, String)>,
    pub vanished_albums: Vec<String>,
    pub searches: Vec<(&'static str, Value)>,
    pub restores: Vec<Value>,
    pub ranges: Vec<String>,
    pub logins: usize,
    next: usize,
}

impl World {
    fn id(&mut self) -> String {
        self.next += 1;
        format!("00000000-0000-4000-8000-{:012}", self.next)
    }

    pub fn add_user(&mut self, email: &str, password: &str) -> String {
        let id = self.id();
        self.users.push(User {
            id: id.clone(),
            email: email.into(),
            password: password.into(),
        });
        id
    }

    pub fn user_by_email(&self, email: &str) -> Option<&User> {
        self.users.iter().find(|u| u.email == email)
    }

    pub fn add_asset(&mut self, owner: &str, created: &str) -> String {
        let id = self.id();
        self.assets.push(Asset {
            id: id.clone(),
            owner: owner.into(),
            checksum: format!("{:040}", self.next),
            live: None,
            name: format!("{id}.jpg"),
            size: 1,
            created: created.into(),
            favorite: false,
            trashed: false,
            file_missing: false,
            city: None,
            country: None,
            person: None,
        });
        id
    }

    pub fn add_trashed_asset(&mut self, owner: &str, created: &str) -> String {
        let id = self.add_asset(owner, created);
        self.asset(&id).trashed = true;
        id
    }

    pub fn add_video(&mut self, owner: &str, created: &str) -> String {
        let id = self.add_asset(owner, created);
        let checksum = format!("{:040}", self.next + 1);
        let asset = self.asset(&id);
        asset.name = format!("{id}.mov");
        asset.size = 12;
        asset.checksum = checksum;
        id
    }

    pub fn add_person(&mut self, owner: &str, name: &str) -> String {
        let id = self.id();
        self.people.push(Person {
            id: id.clone(),
            owner: owner.into(),
            name: name.into(),
            file_missing: false,
        });
        id
    }

    pub fn asset(&mut self, id: &str) -> &mut Asset {
        self.assets.iter_mut().find(|a| a.id == id).unwrap()
    }

    /// 让这一条的文件不在盘上（Immich 的行还在）：取字节回 404。
    pub fn remove_asset_file(&mut self, id: &str) {
        self.asset(id).file_missing = true;
    }

    pub fn remove_person_file(&mut self, id: &str) {
        self.people
            .iter_mut()
            .find(|p| p.id == id)
            .unwrap()
            .file_missing = true;
    }

    pub fn place_of(&mut self, id: &str, city: &str, country: &str) {
        let asset = self.asset(id);
        asset.city = Some(city.into());
        asset.country = Some(country.into());
    }

    pub fn face_of(&mut self, id: &str, person: &str) {
        self.asset(id).person = Some(person.into());
    }

    pub fn add_album(&mut self, name: &str, owner: &str, members: &[&str]) -> String {
        let id = self.id();
        let members: Vec<String> = members.iter().map(|m| (*m).to_owned()).collect();
        self.albums.push((
            id.clone(),
            name.into(),
            std::iter::once(owner.to_owned()).chain(members).collect(),
            vec![],
        ));
        id
    }

    pub fn vanish_album(&mut self, id: &str) {
        self.vanished_albums.push(id.into());
    }
}

pub type Shared = Arc<Mutex<World>>;

fn deny() -> Response {
    (StatusCode::UNAUTHORIZED, "Invalid API key").into_response()
}

/// 请求是谁：`x-api-key` 或会话令牌。
fn caller(world: &World, headers: &HeaderMap) -> Option<String> {
    if let Some(key) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        return world.keys.get(key).cloned();
    }
    let token = headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")?;
    world.sessions.get(token).cloned()
}

macro_rules! who {
    ($world:expr, $headers:expr) => {
        match caller(&$world, &$headers) {
            Some(id) => id,
            None => return deny(),
        }
    };
}

/// Immich 对象级访问检查的失败：看不到和不存在都是这个 400。
fn not_allowed(permission: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        format!("Not found or no {permission} access"),
    )
        .into_response()
}

fn is_partner(world: &World, owner: &str, me: &str) -> bool {
    world
        .partners
        .iter()
        .any(|(from, to)| from == owner && to == me)
}

/// 看得到：自己的、partner 给我的、放进我在的相册的。和 Immich 的 `AssetRead`/`AssetView` 一致。
fn readable(world: &World, me: &str, asset: &Asset) -> bool {
    asset.owner == me
        || is_partner(world, &asset.owner, me)
        || world
            .albums
            .iter()
            .any(|album| album.2.iter().any(|user| user == me) && album.3.contains(&asset.id))
}

/// 找不到、看不到、不是主人（`edit`）都回 Immich 的 400，返回下标。
/// 失败直接带 `Response`（128 字节），比每个调用点再包一层简单。
#[allow(clippy::result_large_err)]
fn visible_asset(
    world: &World,
    me: &str,
    id: &str,
    edit: bool,
    permission: &str,
) -> Result<usize, Response> {
    let index = world
        .assets
        .iter()
        .position(|asset| asset.id == id)
        .ok_or_else(|| not_allowed(permission))?;
    let asset = &world.assets[index];
    let allowed = if edit {
        asset.owner == me
    } else {
        readable(world, me, asset)
    };
    if allowed {
        Ok(index)
    } else {
        Err(not_allowed(permission))
    }
}

/// 字节回包：`Range` 命中回 206 前四个字节并带上游的 `x-seen-range`，越界回 416。
fn bytes_response(headers: &HeaderMap, body: &str, content_type: &str) -> Response {
    let out = Response::builder().header("accept-ranges", "bytes");
    if headers.get("range").is_some_and(|v| *v == "bytes=999-") {
        return out
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header("content-range", format!("bytes */{}", body.len()))
            .body(Body::from(String::new()))
            .unwrap();
    }
    if let Some(range) = headers.get("range") {
        return out
            .status(StatusCode::PARTIAL_CONTENT)
            .header("content-range", format!("bytes 0-3/{}", body.len()))
            .header("x-seen-range", range.to_str().unwrap())
            .header("content-type", content_type)
            .body(Body::from(body[..4].to_owned()))
            .unwrap();
    }
    out.status(StatusCode::OK)
        .header("content-type", content_type)
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn ping() -> Json<Value> {
    Json(json!({"res": "pong"}))
}

async fn login(State(w): State<Shared>, Json(body): Json<Value>) -> Response {
    let mut world = w.lock().unwrap();
    world.logins += 1;
    let found = world
        .users
        .iter()
        .find(|u| u.email == body["email"] && u.password == body["password"])
        .cloned();
    let Some(user) = found else {
        return (StatusCode::UNAUTHORIZED, "Incorrect email or password").into_response();
    };
    let token = format!("session-{}", world.sessions.len() + 1);
    world.sessions.insert(token.clone(), user.id);
    (StatusCode::CREATED, Json(json!({"accessToken": token}))).into_response()
}

async fn logout(State(w): State<Shared>, headers: HeaderMap) -> Response {
    let mut world = w.lock().unwrap();
    let token = headers["authorization"]
        .to_str()
        .unwrap()
        .trim_start_matches("Bearer ");
    world.sessions.remove(token);
    Json(json!({"successful": true})).into_response()
}

fn user_json(user: &User) -> Value {
    json!({"id": user.id, "email": user.email, "name": user.email})
}

async fn admin_users(State(w): State<Shared>, headers: HeaderMap) -> Response {
    let world = w.lock().unwrap();
    let me = who!(world, headers);
    if me != world.users[0].id {
        return (StatusCode::FORBIDDEN, "admin only").into_response();
    }
    Json(world.users.iter().map(user_json).collect::<Vec<_>>()).into_response()
}

async fn create_user(
    State(w): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let mut world = w.lock().unwrap();
    let me = who!(world, headers);
    assert_eq!(me, world.users[0].id, "只有管理员建号");
    let id = world.add_user(
        body["email"].as_str().unwrap(),
        body["password"].as_str().unwrap(),
    );
    let user = world.users.iter().find(|u| u.id == id).unwrap();
    (StatusCode::CREATED, Json(user_json(user))).into_response()
}

async fn update_user(
    State(w): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let mut world = w.lock().unwrap();
    let me = who!(world, headers);
    assert_eq!(me, world.users[0].id, "只有管理员改密码");
    let user = world.users.iter_mut().find(|u| u.id == id).unwrap();
    user.password = body["password"].as_str().unwrap().into();
    let user = user.clone();
    Json(user_json(&user)).into_response()
}

async fn create_key(State(w): State<Shared>, headers: HeaderMap) -> Response {
    let mut world = w.lock().unwrap();
    let me = who!(world, headers);
    let secret = format!("key-{}", world.keys.len() + 1);
    world.keys.insert(secret.clone(), me);
    (StatusCode::CREATED, Json(json!({"secret": secret}))).into_response()
}

async fn me(State(w): State<Shared>, headers: HeaderMap) -> Response {
    let world = w.lock().unwrap();
    let me = who!(world, headers);
    let user = world.users.iter().find(|u| u.id == me).unwrap();
    Json(user_json(user)).into_response()
}

async fn users(State(w): State<Shared>, headers: HeaderMap) -> Response {
    let world = w.lock().unwrap();
    who!(world, headers);
    Json(world.users.iter().map(user_json).collect::<Vec<_>>()).into_response()
}

async fn upload(State(w): State<Shared>, headers: HeaderMap, mut form: Multipart) -> Response {
    let mut fields = HashMap::new();
    let mut size = 0;
    let mut bytes = Vec::new();
    while let Some(field) = form.next_field().await.unwrap() {
        let name = field.name().unwrap().to_owned();
        let data = field.bytes().await.unwrap();
        if name == "assetData" {
            size = data.len();
            bytes = data.to_vec();
        } else {
            fields.insert(name, String::from_utf8(data.to_vec()).unwrap());
        }
    }
    use base64::Engine;
    use sha1::Digest;
    let checksum = hex::encode(sha1::Sha1::digest(&bytes));
    let header = headers["x-immich-checksum"].to_str().unwrap();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(header)
        .unwrap();
    assert_eq!(
        hex::encode(decoded),
        checksum,
        "x-immich-checksum 是字节的 SHA-1"
    );
    let mut world = w.lock().unwrap();
    let owner = who!(world, headers);
    if let Some(found) = world
        .assets
        .iter()
        .find(|a| a.owner == owner && a.checksum == checksum)
    {
        return Json(json!({"id": found.id, "status": "duplicate"})).into_response();
    }
    let id = world.id();
    world.assets.push(Asset {
        id: id.clone(),
        owner,
        checksum,
        live: fields.get("livePhotoVideoId").cloned(),
        name: fields["filename"].clone(),
        size,
        created: fields["fileCreatedAt"].clone(),
        favorite: false,
        trashed: false,
        file_missing: false,
        city: None,
        country: None,
        person: None,
    });
    (
        StatusCode::CREATED,
        Json(json!({"id": id, "status": "created"})),
    )
        .into_response()
}

async fn bulk_check(
    State(w): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let world = w.lock().unwrap();
    let owner = who!(world, headers);
    let results: Vec<Value> = body["assets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            let hit = world
                .assets
                .iter()
                .find(|a| a.owner == owner && a.checksum.eq_ignore_ascii_case(item["checksum"].as_str().unwrap()));
            match hit {
                Some(a) => json!({"id": item["id"], "action": "reject", "reason": "duplicate", "assetId": a.id}),
                None => json!({"id": item["id"], "action": "accept"}),
            }
        })
        .collect();
    Json(json!({"results": results})).into_response()
}

/// 自己的加上 partner 共享给我的。
fn visible<'a>(world: &'a World, me: &str) -> impl Iterator<Item = &'a Asset> {
    world.assets.iter().filter(move |a| {
        a.owner == me
            || world
                .partners
                .iter()
                .any(|(from, to)| from == &a.owner && to == me)
    })
}

async fn buckets(State(w): State<Shared>, headers: HeaderMap) -> Response {
    let world = w.lock().unwrap();
    let me = who!(world, headers);
    let mut counts: Vec<(String, usize)> = Vec::new();
    for asset in visible(&world, &me) {
        let month = format!("{}-01T00:00:00.000Z", &asset.created[..7]);
        match counts.iter_mut().find(|(m, _)| *m == month) {
            Some((_, n)) => *n += 1,
            None => counts.push((month, 1)),
        }
    }
    counts.sort_by(|a, b| b.0.cmp(&a.0));
    let list: Vec<Value> = counts
        .iter()
        .map(|(m, n)| json!({"timeBucket": m, "count": n}))
        .collect();
    Json(list).into_response()
}

async fn bucket(
    State(w): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let world = w.lock().unwrap();
    let me = who!(world, headers);
    let month = &q["timeBucket"][..7];
    let assets: Vec<&Asset> = visible(&world, &me)
        .filter(|a| a.created.starts_with(month))
        .collect();
    let col = |f: &dyn Fn(&Asset) -> Value| -> Value {
        assets.iter().map(|a| f(a)).collect::<Vec<_>>().into()
    };
    Json(json!({
        "id": col(&|a| a.id.clone().into()),
        "isImage": col(&|a| (!a.name.ends_with(".mov")).into()),
        "fileCreatedAt": col(&|a| a.created.clone().into()),
        "ratio": col(&|_| 1.5.into()),
        "duration": col(&|a| if a.name.ends_with(".mov") { 3000.into() } else { Value::Null }),
        "livePhotoVideoId": col(&|a| a.live.clone().map(Value::from).unwrap_or(Value::Null)),
        "isFavorite": col(&|a| a.favorite.into()),
        "ownerId": col(&|a| a.owner.clone().into()),
        "thumbhash": col(&|_| Value::Null),
    }))
    .into_response()
}

fn album_json(album: &(String, String, Vec<String>, Vec<String>)) -> Value {
    let users: Vec<Value> = album
        .2
        .iter()
        .enumerate()
        .map(|(i, id)| json!({"role": if i == 0 { "owner" } else { "editor" }, "user": {"id": id}}))
        .collect();
    json!({
        "id": album.0, "albumName": album.1, "albumUsers": users, "assetCount": album.3.len(),
        "shared": album.2.len() > 1, "albumThumbnailAssetId": album.3.first(),
        "updatedAt": "2026-11-20T08:00:00.000Z",
    })
}

async fn albums(State(w): State<Shared>, headers: HeaderMap) -> Response {
    let world = w.lock().unwrap();
    let me = who!(world, headers);
    let list: Vec<Value> = world
        .albums
        .iter()
        .filter(|a| a.2.contains(&me) && !world.vanished_albums.contains(&a.0))
        .map(album_json)
        .collect();
    Json(list).into_response()
}

async fn album_detail(
    State(w): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let world = w.lock().unwrap();
    let me = who!(world, headers);
    let Some(album) = world.albums.iter().find(|a| a.0 == id) else {
        return not_allowed("album.read");
    };
    if !album.2.contains(&me) {
        return not_allowed("album.read");
    }
    // 权限查过之后行没了：Immich 是 `findOrFail` 的 404。
    if world.vanished_albums.contains(&id) {
        return (StatusCode::NOT_FOUND, "").into_response();
    }
    Json(album_json(album)).into_response()
}

fn asset_json(world: &World, asset: &Asset) -> Value {
    let video = asset.name.ends_with(".mov");
    let people: Vec<Value> = asset
        .person
        .iter()
        .filter_map(|id| world.people.iter().find(|person| &person.id == id))
        .filter(|person| !person.name.is_empty())
        .map(|person| json!({"id": person.id, "name": person.name}))
        .collect();
    json!({
        "id": asset.id,
        "type": if video { "VIDEO" } else { "IMAGE" },
        "fileCreatedAt": asset.created,
        "fileModifiedAt": asset.created,
        "createdAt": asset.created,
        "updatedAt": asset.created,
        "ownerId": asset.owner,
        "isFavorite": asset.favorite,
        "livePhotoVideoId": asset.live,
        "originalFileName": asset.name,
        "originalPath": format!("upload/{}", asset.name),
        "width": 3000,
        "height": 2000,
        "duration": if video { Some(3000) } else { None::<u32> },
        "thumbhash": Value::Null,
        "exifInfo": {
            "exifImageWidth": 3000,
            "exifImageHeight": 2000,
            "fileSizeInByte": asset.size,
            "city": asset.city,
            "country": asset.country,
        },
        "people": people,
    })
}

async fn search_impl(shared: Shared, headers: HeaderMap, body: Value, smart: bool) -> Response {
    let mut world = shared.lock().unwrap();
    let me = who!(world, headers);
    world
        .searches
        .push((if smart { "smart" } else { "metadata" }, body.clone()));
    let album_ids: Vec<String> = body["albumIds"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|id| id.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let mut pool: Vec<&Asset> = if album_ids.is_empty() {
        visible(&world, &me).collect()
    } else {
        for id in &album_ids {
            match world.albums.iter().find(|album| &album.0 == id) {
                Some(album) if album.2.contains(&me) => {}
                _ => return not_allowed("album.read"),
            }
        }
        world
            .albums
            .iter()
            .filter(|album| album_ids.contains(&album.0))
            .flat_map(|album| album.3.iter())
            .filter_map(|id| world.assets.iter().find(|asset| &asset.id == id))
            .collect()
    };
    // 回收站里的不进搜索结果（Immich 默认按 deletedAt 为空过滤）。
    pool.retain(|asset| !asset.trashed);
    if let Some(ids) = body["personIds"].as_array() {
        pool.retain(|asset| {
            asset
                .person
                .as_ref()
                .is_some_and(|person| ids.contains(&json!(person)))
        });
    }
    if let Some(query) = body["query"].as_str() {
        let query = query.to_lowercase();
        pool.retain(|asset| asset.name.to_lowercase().contains(&query));
    }
    if let Some(city) = body["city"].as_str() {
        pool.retain(|asset| asset.city.as_deref() == Some(city));
    }
    if let Some(country) = body["country"].as_str() {
        pool.retain(|asset| asset.country.as_deref() == Some(country));
    }
    if let Some(after) = body["takenAfter"].as_str() {
        pool.retain(|asset| asset.created.as_str() >= after);
    }
    if let Some(before) = body["takenBefore"].as_str() {
        pool.retain(|asset| asset.created.as_str() <= before);
    }
    if let Some(kind) = body["type"].as_str() {
        pool.retain(|asset| asset.name.ends_with(".mov") == (kind == "VIDEO"));
    }
    if let Some(favorite) = body["isFavorite"].as_bool() {
        pool.retain(|asset| asset.favorite == favorite);
    }
    pool.sort_by(|a, b| b.created.cmp(&a.created));
    let total = pool.len();
    let page = body["page"].as_u64().unwrap_or(1).max(1);
    let size = body["size"].as_u64().unwrap_or(250).max(1);
    let start = ((page - 1) * size) as usize;
    let items: Vec<Value> = pool
        .iter()
        .skip(start)
        .take(size as usize)
        .map(|asset| asset_json(&world, asset))
        .collect();
    let next = (start + items.len() < total).then(|| (page + 1).to_string());
    Json(json!({
        "albums": {"total": 0, "count": 0, "items": [], "facets": []},
        "assets": {
            "total": total, "count": items.len(), "items": items, "facets": [],
            "nextPage": next, "nextCursor": null,
        },
    }))
    .into_response()
}

async fn search(
    OriginalUri(uri): OriginalUri,
    State(w): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    search_impl(w, headers, body, uri.path().ends_with("/smart")).await
}

fn person_readable(world: &World, me: &str, person: &Person) -> bool {
    person.owner == me || is_partner(world, &person.owner, me)
}

async fn people(State(w): State<Shared>, headers: HeaderMap) -> Response {
    let world = w.lock().unwrap();
    let me = who!(world, headers);
    let list: Vec<Value> = world
        .people
        .iter()
        .filter(|person| person_readable(&world, &me, person))
        .map(|person| {
            json!({
                "id": person.id, "name": person.name, "isHidden": false, "birthDate": null,
                "thumbnailPath": format!("/api/people/{}/thumbnail", person.id),
            })
        })
        .collect();
    Json(json!({"total": list.len(), "people": list, "hidden": 0})).into_response()
}

async fn person_thumbnail(
    State(w): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let world = w.lock().unwrap();
    let me = who!(world, headers);
    let Some(person) = world.people.iter().find(|person| person.id == id) else {
        return not_allowed("person.read");
    };
    if !person_readable(&world, &me, person) {
        return not_allowed("person.read");
    }
    if person.file_missing {
        return (StatusCode::NOT_FOUND, "").into_response();
    }
    bytes_response(&headers, &format!("face-of-{id}"), "image/jpeg")
}

async fn asset_detail(
    State(w): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let world = w.lock().unwrap();
    let me = who!(world, headers);
    match visible_asset(&world, &me, &id, false, "asset.read") {
        Ok(index) => Json(asset_json(&world, &world.assets[index])).into_response(),
        Err(response) => response,
    }
}

async fn original(State(w): State<Shared>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let world = w.lock().unwrap();
    let me = who!(world, headers);
    let index = match visible_asset(&world, &me, &id, false, "asset.download") {
        Ok(index) => index,
        Err(response) => return response,
    };
    if world.assets[index].file_missing {
        return (StatusCode::NOT_FOUND, "").into_response();
    }
    bytes_response(&headers, &format!("original-of-{id}"), "image/jpeg")
}

async fn playback(State(w): State<Shared>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let mut world = w.lock().unwrap();
    let me = who!(world, headers);
    if let Some(range) = headers.get("range") {
        world.ranges.push(range.to_str().unwrap().to_owned());
    }
    let index = match visible_asset(&world, &me, &id, false, "asset.view") {
        Ok(index) => index,
        Err(response) => return response,
    };
    let asset = &world.assets[index];
    if !asset.name.ends_with(".mov") || asset.file_missing {
        return (
            StatusCode::NOT_FOUND,
            "Asset not found or asset is not a video",
        )
            .into_response();
    }
    bytes_response(&headers, &format!("video-bytes-of-{id}"), "video/mp4")
}

async fn restore(State(w): State<Shared>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let mut world = w.lock().unwrap();
    let me = who!(world, headers);
    world.restores.push(body.clone());
    let ids: Vec<String> = body["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect();
    let mut indexes = Vec::new();
    for id in &ids {
        match visible_asset(&world, &me, id, true, "asset.delete") {
            Ok(index) => indexes.push(index),
            Err(response) => return response,
        }
    }
    for index in indexes {
        world.assets[index].trashed = false;
    }
    Json(json!({"count": ids.len()})).into_response()
}

async fn create_album(
    State(w): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let mut world = w.lock().unwrap();
    let me = who!(world, headers);
    let members: Vec<String> = body["albumUsers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["userId"].as_str().unwrap().to_owned())
        .collect();
    let refs: Vec<&str> = members.iter().map(String::as_str).collect();
    let id = world.add_album(body["albumName"].as_str().unwrap(), &me, &refs);
    let assets: Vec<String> = body["assetIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().to_owned())
        .collect();
    let album = world.albums.iter_mut().find(|a| a.0 == id).unwrap();
    album.3 = assets;
    let album = album.clone();
    (StatusCode::CREATED, Json(album_json(&album))).into_response()
}

async fn favorite(
    State(w): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let mut world = w.lock().unwrap();
    let me = who!(world, headers);
    let ids = body["ids"].as_array().unwrap();
    // 和 Immich 一样先查整批的权限，有一条不行整批不动。
    let mine = |a: &Asset| ids.contains(&json!(a.id)) && a.owner == me;
    if world.assets.iter().filter(|a| mine(a)).count() != ids.len() {
        return (
            StatusCode::BAD_REQUEST,
            "Not found or no asset.update access",
        )
            .into_response();
    }
    for asset in world.assets.iter_mut().filter(|a| mine(a)) {
        asset.favorite = body["isFavorite"].as_bool().unwrap();
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn thumbnail(
    State(w): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let world = w.lock().unwrap();
    let me = who!(world, headers);
    let index = match visible_asset(&world, &me, &id, false, "asset.view") {
        Ok(index) => index,
        Err(response) => return response,
    };
    if world.assets[index].file_missing {
        return (StatusCode::NOT_FOUND, "").into_response();
    }
    let body = format!("thumbnail-of-{id}");
    let etag = format!("\"{id}\"");
    if headers.get("if-none-match").is_some_and(|v| *v == *etag) {
        return (StatusCode::NOT_MODIFIED, [("etag", etag)]).into_response();
    }
    bytes_response(&headers, &body, "image/webp")
}

pub async fn start(world: Shared) -> String {
    let app = Router::new()
        .route("/api/server/ping", get(ping))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/admin/users", get(admin_users).post(create_user))
        .route("/api/admin/users/{id}", put(update_user))
        .route("/api/api-keys", post(create_key))
        .route("/api/users/me", get(me))
        .route("/api/users", get(users))
        .route("/api/assets", post(upload).put(favorite))
        .route("/api/assets/bulk-upload-check", post(bulk_check))
        .route("/api/assets/{id}", get(asset_detail))
        .route("/api/assets/{id}/thumbnail", get(thumbnail))
        .route("/api/assets/{id}/original", get(original))
        .route("/api/assets/{id}/video/playback", get(playback))
        .route("/api/timeline/buckets", get(buckets))
        .route("/api/timeline/bucket", get(bucket))
        .route("/api/albums", get(albums).post(create_album))
        .route("/api/albums/{id}", get(album_detail))
        .route("/api/search/smart", post(search))
        .route("/api/search/metadata", post(search))
        .route("/api/people", get(people))
        .route("/api/people/{id}/thumbnail", get(person_thumbnail))
        .route("/api/trash/restore/assets", post(restore))
        .with_state(world);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}
