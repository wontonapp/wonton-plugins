//! 集成测试共用：起适配器、假 Immich 和一个替身核心凭据。各测试文件只挑自己用得到的。
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use serde_json::Value;
use sha1::Digest;
use wonton_immich_media::accounts::{Accounts, Owner};
use wonton_immich_media::{App, auth, immich, router};

use crate::fake_immich::{self, Shared, World};

pub const TOKEN: &str = "core-token";
pub const OWNER: &str = "@dad:example.org";
pub const ALICE: &str = "@alice:example.org";

pub struct Rig {
    pub world: Shared,
    pub base: String,
    pub http: reqwest::Client,
    _state: tempfile::TempDir,
}

pub async fn rig() -> Rig {
    let mut world = World::default();
    world.add_user("dad@example.com", "admin-pass");
    let world: Shared = Arc::new(Mutex::new(world));
    let immich_base = fake_immich::start(world.clone()).await;
    let state = tempfile::tempdir().unwrap();
    auth::issue(state.path(), "wonton", TOKEN).unwrap();
    let client = immich::Client::new(immich_base);
    let owner = Owner {
        name: "dad".into(),
        email: "dad@example.com".into(),
        password: "admin-pass".into(),
    };
    let app = App {
        accounts: Arc::new(Accounts::new(state.path(), owner, client.clone())),
        immich: client,
        state: state.path().to_owned(),
        household: Some("family".into()),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router(app)).await.unwrap() });
    Rig {
        world,
        base: format!("http://{addr}"),
        http: reqwest::Client::new(),
        _state: state,
    }
}

impl Rig {
    pub fn req(&self, method: reqwest::Method, path: &str, user: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(TOKEN)
            .header("X-Wonton-User", user)
    }

    pub async fn json(&self, builder: reqwest::RequestBuilder) -> (u16, Value) {
        let response = builder.send().await.unwrap();
        let status = response.status().as_u16();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    /// 先替 alice 在假 Immich 里开好户，返回她的用户 id。
    pub async fn alice(&self) -> String {
        self.json(self.req(reqwest::Method::GET, "/library/timeline", ALICE))
            .await;
        self.world
            .lock()
            .unwrap()
            .user_by_email("alice@wonton.internal")
            .unwrap()
            .id
            .clone()
    }

    pub fn dad(&self) -> String {
        self.world
            .lock()
            .unwrap()
            .user_by_email("dad@example.com")
            .unwrap()
            .id
            .clone()
    }

    /// 走一遍契约的整条上传：开始 → 一次写完 → 收尾。
    pub async fn upload(
        &self,
        user: &str,
        path: &str,
        bytes: &[u8],
        live: Option<&str>,
    ) -> (u16, Value) {
        let body = serde_json::json!({
            "path": path, "size": bytes.len(), "checksum": sha1(bytes),
            "taken_at": 1_763_600_000_000u64, "live_video_checksum": live,
        });
        let (_, started) = self
            .json(
                self.req(reqwest::Method::POST, "/uploads", user)
                    .json(&body),
            )
            .await;
        let id = started["upload_id"].as_str().unwrap().to_owned();
        let patch = self
            .req(reqwest::Method::PATCH, &format!("/uploads/{id}"), user)
            .header("Upload-Offset", "0")
            .body(bytes.to_vec());
        assert_eq!(self.json(patch).await.0, 200);
        self.json(self.req(
            reqwest::Method::POST,
            &format!("/uploads/{id}/finish"),
            user,
        ))
        .await
    }
}

pub fn sha1(bytes: &[u8]) -> String {
    format!("sha1:{}", hex::encode(sha1::Sha1::digest(bytes)))
}

/// 假 Immich 里一个格式合法但没建过的编号。
pub const UNKNOWN: &str = "11111111-1111-4111-8111-111111111111";
