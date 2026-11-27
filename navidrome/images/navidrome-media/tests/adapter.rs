//! 行为测试：起一个照 Navidrome 0.64.2 实测行为写的假上游，经适配器的 HTTP 接口走一遍核心会做的调用。

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};
use wonton_navidrome_media::keys::KeyStore;
use wonton_navidrome_media::routes::{AppState, router};
use wonton_navidrome_media::upstream::Upstream;

const KEY: &str = "core-key";

/// 上游收到的一次 `/rest` 调用。
#[derive(Debug, Clone)]
struct Call {
    method: String,
    query: Vec<(String, String)>,
    user: String,
    range: Option<String>,
}

impl Call {
    fn param(&self, name: &str) -> Vec<&str> {
        self.query
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .collect()
    }
}

#[derive(Default)]
struct Navidrome {
    accounts: Mutex<HashSet<String>>,
    calls: Mutex<Vec<Call>>,
}

impl Navidrome {
    fn calls(&self, method: &str) -> Vec<Call> {
        let calls = self.calls.lock().unwrap();
        calls
            .iter()
            .filter(|c| c.method == method)
            .cloned()
            .collect()
    }
}

fn ok(body: &Value) -> Response {
    let mut envelope = json!({"status": "ok", "version": "1.16.1"});
    envelope
        .as_object_mut()
        .unwrap()
        .extend(body.as_object().unwrap().clone());
    Json(json!({"subsonic-response": envelope})).into_response()
}

fn failed(code: i64, message: &str) -> Response {
    Json(json!({"subsonic-response": {
        "status": "failed", "version": "1.16.1", "error": {"code": code, "message": message}
    }}))
    .into_response()
}

fn song(id: &str) -> Value {
    match id {
        "s1" => json!({"id": "s1", "title": "Harbor Lights", "artist": "Evening Breeze",
            "album": "Summer by the Sea", "track": 1, "duration": 200, "suffix": "flac",
            "contentType": "audio/flac", "bitRate": 900, "coverArt": "mf-s1", "playCount": 2,
            "starred": "2026-01-01T00:00:00Z"}),
        "s2" => json!({"id": "s2", "title": "Low Tide", "artist": "Lin Yi",
            "album": "Summer by the Sea", "track": 2, "duration": 180, "suffix": "mp3",
            "contentType": "audio/mpeg", "bitRate": 320}),
        _ => Value::Null,
    }
}

async fn rest(
    State(upstream): State<Arc<Navidrome>>,
    Path(method): Path<String>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Response {
    let query: Vec<(String, String)> = raw.as_deref().map(url_pairs).unwrap_or_default();
    let user = headers
        .get("remote-user")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let call = Call {
        method: method.clone(),
        query,
        user: user.clone(),
        range: headers
            .get(header::RANGE)
            .map(|v| v.to_str().unwrap().to_owned()),
    };
    upstream.calls.lock().unwrap().push(call.clone());
    if !upstream.accounts.lock().unwrap().contains(&user) {
        return failed(40, "Wrong username or password");
    }
    let id = call
        .param("id")
        .first()
        .map(ToString::to_string)
        .unwrap_or_default();
    match method.as_str() {
        "search3" => ok(&json!({"searchResult3": {"song": [song("s1"), song("s2")]}})),
        "getSong" if song(&id).is_null() => failed(70, "Song not found"),
        "getSong" => ok(&json!({"song": song(&id)})),
        "scrobble" | "savePlayQueue" => ok(&json!({})),
        "stream" => {
            let mut response = (StatusCode::PARTIAL_CONTENT, "abc").into_response();
            let h = response.headers_mut();
            h.insert(header::CONTENT_TYPE, "audio/flac".parse().unwrap());
            h.insert(header::CONTENT_RANGE, "bytes 0-2/10".parse().unwrap());
            response
        }
        _ => failed(0, "unexpected method"),
    }
}

fn url_pairs(raw: &str) -> Vec<(String, String)> {
    raw.split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| {
            let v = percent_encoding::percent_decode_str(&v.replace('+', " "))
                .decode_utf8_lossy()
                .into_owned();
            (k.to_owned(), v)
        })
        .collect()
}

async fn app(State(upstream): State<Arc<Navidrome>>, headers: HeaderMap) -> StatusCode {
    if let Some(user) = headers.get("remote-user").and_then(|v| v.to_str().ok()) {
        upstream.accounts.lock().unwrap().insert(user.to_owned());
    }
    StatusCode::OK
}

struct Harness {
    navidrome: Arc<Navidrome>,
    base: String,
    http: reqwest::Client,
    _keys: tempfile::TempDir,
}

impl Harness {
    async fn start() -> Self {
        let navidrome = Arc::new(Navidrome::default());
        let fake = Router::new()
            .route("/rest/{method}", get(rest))
            .route("/app/", get(app))
            .with_state(navidrome.clone());
        let upstream = serve(fake).await;
        let dir = tempfile::tempdir().unwrap();
        let keys = KeyStore::new(dir.path());
        keys.issue("wonton-core.media.alice", KEY).unwrap();
        let state = AppState {
            upstream: Upstream::new(&upstream),
            keys: Arc::new(keys),
        };
        Self {
            navidrome,
            base: serve(router(state)).await,
            http: reqwest::Client::builder().no_proxy().build().unwrap(),
            _keys: dir,
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(KEY)
            .header("X-Wonton-User", "alice")
    }

    async fn json(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut request = self.request(method, path);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

fn profile(direct: &[&str], max_bitrate: u32) -> Value {
    json!({"direct_play": direct, "max_bitrate": max_bitrate, "hdr": false, "container": []})
}

fn playback(id: &str, profile: &Value) -> Value {
    json!({"position_ms": 0, "audio": null, "subtitle": null, "device": "d1", "person": "alice",
        "item": {"id": id, "kind": "audio", "title": "", "artist": null, "album": null,
            "artwork_url": null, "duration_ms": null, "series": null, "index": null,
            "played": false, "starred": false},
        "profile": profile})
}

#[tokio::test]
async fn requests_without_a_core_key_are_refused_but_the_probe_is_open() {
    let h = Harness::start().await;
    let response = h
        .http
        .get(format!("{}/items", h.base))
        .bearer_auth("guess")
        .header("X-Wonton-User", "alice")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["code"], "authentication_required");

    let probe: Value = h
        .http
        .get(format!("{}/_wonton/capabilities", h.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        probe,
        json!({"operations": ["download", "playback", "list"], "kinds": ["audio"]})
    );
    assert!(h.navidrome.calls("search3").is_empty());
}

#[tokio::test]
async fn a_person_without_an_account_gets_one_and_the_list_carries_their_songs() {
    let h = Harness::start().await;
    let (status, page) = h
        .json(reqwest::Method::GET, "/items?kind=audio&q=summer", None)
        .await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["items"][0]["id"], "s1");
    assert_eq!(page["items"][0]["duration_ms"], 200_000);
    assert_eq!(page["items"][0]["played"], true);
    assert_eq!(page["items"][0]["starred"], true);
    assert_eq!(page["items"][1]["starred"], false);
    assert!(page.get("cursor").is_none());

    let searches = h.navidrome.calls("search3");
    assert_eq!(searches.len(), 2, "第一次 40，建号后重试");
    assert!(
        searches
            .iter()
            .all(|c| c.user == "alice" && c.param("query") == ["summer"])
    );

    let (_, filtered) = h
        .json(reqwest::Method::GET, "/items?artist=lin%20yi", None)
        .await;
    let ids: Vec<&Value> = filtered["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| &i["id"])
        .collect();
    assert_eq!(ids, [&json!("s2")]);

    let (_, photos) = h
        .json(reqwest::Method::GET, "/items?kind=photo", None)
        .await;
    assert_eq!(photos["items"], json!([]));
}

#[tokio::test]
async fn the_plan_plays_what_the_device_takes_and_transcodes_the_rest() {
    let h = Harness::start().await;
    h.navidrome.accounts.lock().unwrap().insert("alice".into());

    let (_, direct) = h
        .json(
            reqwest::Method::POST,
            "/playback",
            Some(playback("s1", &profile(&["audio/flac"], 0))),
        )
        .await;
    assert_eq!(direct["stream"], json!({"direct": "/items/s1/stream"}));
    assert_eq!(direct["seekable"], true);
    assert_eq!(direct["session"], "s1");
    assert_eq!(direct["duration_ms"], 200_000);

    let (_, capped) = h
        .json(
            reqwest::Method::POST,
            "/playback",
            Some(playback(
                "s1",
                &profile(&["audio/flac", "audio/mpeg"], 128_000),
            )),
        )
        .await;
    assert_eq!(
        capped["stream"],
        json!({"transcoded": "/items/s1/stream?format=mp3&max_bitrate=128"})
    );
    assert_eq!(capped["seekable"], false);

    let (status, refused) = h
        .json(
            reqwest::Method::POST,
            "/playback",
            Some(playback("s1", &profile(&["video/mp4"], 0))),
        )
        .await;
    assert_eq!(
        (status, refused["code"].as_str()),
        (422, Some("unsupported_by_device"))
    );

    let (status, gone) = h
        .json(
            reqwest::Method::POST,
            "/playback",
            Some(playback("nope", &profile(&[], 0))),
        )
        .await;
    assert_eq!((status, gone["code"].as_str()), (404, Some("media_gone")));
}

#[tokio::test]
async fn the_stream_carries_range_through_both_ways() {
    let h = Harness::start().await;
    h.navidrome.accounts.lock().unwrap().insert("alice".into());
    let response = h
        .request(reqwest::Method::GET, "/items/s1/stream")
        .header("Range", "bytes=0-2")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 206);
    assert_eq!(response.headers()["content-range"], "bytes 0-2/10");
    assert_eq!(response.text().await.unwrap(), "abc");

    h.request(
        reqwest::Method::GET,
        "/items/s1/stream?format=mp3&max_bitrate=128",
    )
    .send()
    .await
    .unwrap();
    let streams = h.navidrome.calls("stream");
    assert_eq!(streams[0].range.as_deref(), Some("bytes=0-2"));
    assert_eq!(streams[0].param("format"), ["raw"]);
    assert_eq!(streams[1].param("format"), ["mp3"]);
    assert_eq!(streams[1].param("maxBitRate"), ["128"]);
}

#[tokio::test]
async fn plays_count_once_listened_long_enough_and_the_queue_is_saved_upstream() {
    let h = Harness::start().await;
    h.navidrome.accounts.lock().unwrap().insert("alice".into());
    let progress = json!({"position_ms": 5_000, "wall_clock": 1});
    let (status, _) = h
        .json(
            reqwest::Method::POST,
            "/playback/s1/progress",
            Some(progress),
        )
        .await;
    assert_eq!(status, 204);
    let skipped = json!({"position_ms": 10_000, "reason": "user"});
    h.json(reqwest::Method::POST, "/playback/s1/stopped", Some(skipped))
        .await;
    let listened = json!({"position_ms": 150_000, "reason": "user"});
    h.json(
        reqwest::Method::POST,
        "/playback/s1/stopped",
        Some(listened),
    )
    .await;
    let finished = json!({"position_ms": 0, "reason": "finished"});
    h.json(
        reqwest::Method::POST,
        "/playback/s2/stopped",
        Some(finished),
    )
    .await;

    let scrobbles: Vec<(String, String)> = h
        .navidrome
        .calls("scrobble")
        .iter()
        .map(|c| {
            (
                c.param("id")[0].to_owned(),
                c.param("submission")[0].to_owned(),
            )
        })
        .collect();
    let expected = [("s1", "false"), ("s1", "true"), ("s2", "true")];
    assert_eq!(
        scrobbles,
        expected.map(|(a, b)| (a.to_owned(), b.to_owned()))
    );

    let queue = json!({"entries": [playback("s1", &json!({}))["item"], playback("s2", &json!({}))["item"]],
        "current": 1, "position_ms": 42_000, "mode": "shuffle"});
    let (status, _) = h
        .json(reqwest::Method::PUT, "/playback/s2/queue", Some(queue))
        .await;
    assert_eq!(status, 204);
    let saved = &h.navidrome.calls("savePlayQueue")[0];
    assert_eq!(saved.param("id"), ["s1", "s2"]);
    assert_eq!(saved.param("current"), ["s2"]);
    assert_eq!(saved.param("position"), ["42000"]);
}
