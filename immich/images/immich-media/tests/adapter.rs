//! 适配器的契约行为：核心照 media_source 契约发请求，断言落到了谁的 Immich 库里、回了什么类别。
//! 假 Immich 见 `fake_immich`，起的架子见 `common`。

mod common;
mod fake_immich;

use serde_json::{Value, json};

use common::{ALICE, OWNER, TOKEN, rig, sha1};

#[tokio::test]
async fn interrupted_upload_resumes_from_the_offset_and_lands_in_the_persons_own_library() {
    let rig = rig().await;
    let bytes = b"0123456789";
    let body = json!({"path": "DCIM/IMG_1.JPG", "size": 10, "checksum": sha1(bytes), "taken_at": 1_763_600_000_000u64});
    let (status, started) = rig
        .json(
            rig.req(reqwest::Method::POST, "/uploads", ALICE)
                .json(&body),
        )
        .await;
    assert_eq!((status, started["upload_offset"].clone()), (200, json!(0)));
    let id = started["upload_id"].as_str().unwrap().to_owned();
    let path = format!("/uploads/{id}");

    let first = rig
        .req(reqwest::Method::PATCH, &path, ALICE)
        .header("Upload-Offset", "0")
        .body(&bytes[..4]);
    assert_eq!(rig.json(first).await, (200, json!({"upload_offset": 4})));
    // 断线后核心拿着旧偏移重发：回 409 和真实偏移，不重复写。
    let stale = rig
        .req(reqwest::Method::PATCH, &path, ALICE)
        .header("Upload-Offset", "0")
        .body(&bytes[..4]);
    assert_eq!(rig.json(stale).await, (409, json!({"upload_offset": 4})));
    let head = rig
        .req(reqwest::Method::HEAD, &path, ALICE)
        .send()
        .await
        .unwrap();
    assert_eq!(head.headers()["upload-offset"], "4");
    let rest = rig
        .req(reqwest::Method::PATCH, &path, ALICE)
        .header("Upload-Offset", "4")
        .body(&bytes[4..]);
    assert_eq!(rig.json(rest).await.0, 200);

    let (status, finished) = rig
        .json(rig.req(reqwest::Method::POST, &format!("{path}/finish"), ALICE))
        .await;
    assert_eq!(status, 200, "{finished}");
    assert_eq!(finished["duplicate"], false);
    let world = rig.world.lock().unwrap();
    let alice = world
        .user_by_email("alice@wonton.internal")
        .expect("第一次替家人办事时开户");
    let asset = world
        .assets
        .iter()
        .find(|a| a.id == finished["item_id"])
        .unwrap();
    assert_eq!(
        (asset.owner.as_str(), asset.size, asset.name.as_str()),
        (alice.id.as_str(), 10, "IMG_1.JPG")
    );
    assert!(
        asset.created.starts_with("2025-11-20"),
        "拍摄时间进 fileCreatedAt：{}",
        asset.created
    );
}

#[tokio::test]
async fn already_backed_up_items_are_reported_by_checksum_and_a_late_duplicate_is_not_a_failure() {
    let rig = rig().await;
    let (_, first) = rig.upload(ALICE, "a.jpg", b"same bytes", None).await;
    let body = json!({"checksums": [sha1(b"same bytes"), sha1(b"new"), "sha256:abc"]});
    let (status, checked) = rig
        .json(
            rig.req(reqwest::Method::POST, "/uploads/check", ALICE)
                .json(&body),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        checked,
        json!({"existing": [{"checksum": sha1(b"same bytes"), "item_id": first["item_id"]}]})
    );
    // 主人库里没有这张：去重按人，不按全家。
    let (_, mine) = rig
        .json(
            rig.req(reqwest::Method::POST, "/uploads/check", OWNER)
                .json(&body),
        )
        .await;
    assert_eq!(mine, json!({"existing": []}));

    let (status, again) = rig.upload(ALICE, "copy.jpg", b"same bytes", None).await;
    assert_eq!(
        (status, again["duplicate"].clone(), again["item_id"].clone()),
        (200, json!(true), first["item_id"].clone())
    );
}

#[tokio::test]
async fn live_photo_still_is_linked_to_its_video_and_refused_before_the_video_arrives() {
    let rig = rig().await;
    let video = b"motion part".as_slice();
    let (status, early) = rig
        .upload(ALICE, "IMG_2.HEIC", b"still", Some(&sha1(video)))
        .await;
    assert_eq!((status, early["code"].clone()), (400, json!("protocol")));

    let (_, moving) = rig.upload(ALICE, "IMG_2.mov", video, None).await;
    let (status, still) = rig
        .upload(ALICE, "IMG_2.HEIC", b"still", Some(&sha1(video)))
        .await;
    assert_eq!(status, 200, "{still}");
    let world = rig.world.lock().unwrap();
    let asset = world
        .assets
        .iter()
        .find(|a| a.id == still["item_id"])
        .unwrap();
    assert_eq!(asset.live.as_deref(), moving["item_id"].as_str());
}

#[tokio::test]
async fn corrupted_bytes_are_refused_and_the_upload_is_discarded() {
    let rig = rig().await;
    let body = json!({"path": "b.jpg", "size": 3, "checksum": sha1(b"abc"), "taken_at": 1});
    let undated = json!({"path": "b.jpg", "size": 3});
    let (status, refused) = rig
        .json(
            rig.req(reqwest::Method::POST, "/uploads", ALICE)
                .json(&undated),
        )
        .await;
    assert_eq!(
        (status, refused["code"].clone()),
        (400, json!("protocol")),
        "没有拍摄时间不当成今天拍的"
    );
    let (_, started) = rig
        .json(
            rig.req(reqwest::Method::POST, "/uploads", ALICE)
                .json(&body),
        )
        .await;
    let path = format!("/uploads/{}", started["upload_id"].as_str().unwrap());
    let patch = rig
        .req(reqwest::Method::PATCH, &path, ALICE)
        .header("Upload-Offset", "0")
        .body("abd");
    rig.json(patch).await;
    let (status, failed) = rig
        .json(rig.req(reqwest::Method::POST, &format!("{path}/finish"), ALICE))
        .await;
    assert_eq!((status, failed["code"].clone()), (400, json!("protocol")));
    let (status, gone) = rig
        .json(rig.req(reqwest::Method::POST, &format!("{path}/finish"), ALICE))
        .await;
    assert_eq!((status, gone["code"].clone()), (404, json!("media_gone")));
    assert!(rig.world.lock().unwrap().assets.is_empty());
}

#[tokio::test]
async fn requests_without_the_core_credential_or_a_person_are_refused() {
    let rig = rig().await;
    let no_token = rig
        .http
        .get(format!("{}/library/timeline", rig.base))
        .header("X-Wonton-User", ALICE);
    assert_eq!(
        rig.json(no_token).await.1["code"],
        "authentication_required"
    );
    let wrong = rig
        .http
        .get(format!("{}/library/timeline", rig.base))
        .bearer_auth("guess")
        .header("X-Wonton-User", ALICE);
    assert_eq!(rig.json(wrong).await.0, 401);
    let nobody = rig
        .http
        .get(format!("{}/library/timeline", rig.base))
        .bearer_auth(TOKEN);
    assert_eq!(
        rig.json(nobody).await,
        (
            400,
            json!({"code": "protocol", "message": "缺少 X-Wonton-User"})
        )
    );
    assert!(
        rig.world
            .lock()
            .unwrap()
            .user_by_email("alice@wonton.internal")
            .is_none()
    );
}

#[tokio::test]
async fn timeline_shows_partner_photos_with_their_owner_and_albums_mark_the_household() {
    let rig = rig().await;
    rig.upload(ALICE, "mine.jpg", b"alice's", None).await;
    let (dad_id, alice_id, family_id) = {
        let mut world = rig.world.lock().unwrap();
        let dad = world.user_by_email("dad@example.com").unwrap().id.clone();
        let alice = world
            .user_by_email("alice@wonton.internal")
            .unwrap()
            .id
            .clone();
        let family = world.add_user("family@wonton.internal", "x");
        world.add_asset(&dad, "2025-11-03T10:00:00.000Z");
        world.add_asset(&dad, "2025-10-03T10:00:00.000Z");
        world.partners.push((dad.clone(), alice.clone()));
        world.add_album("家里", &family, &[&alice, &dad]);
        (dad, alice, family)
    };
    let _ = (dad_id, family_id);

    let (_, timeline) = rig
        .json(rig.req(reqwest::Method::GET, "/library/timeline", ALICE))
        .await;
    assert_eq!(
        timeline,
        json!({"buckets": [{"month": "2025-11", "count": 2}, {"month": "2025-10", "count": 1}]})
    );
    let (_, month) = rig
        .json(rig.req(reqwest::Method::GET, "/library/timeline/2025-11", ALICE))
        .await;
    let owners: Vec<Value> = month["assets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["owner"].clone())
        .collect();
    assert!(
        owners.contains(&json!("dad")) && owners.contains(&Value::Null),
        "{month}"
    );
    assert_eq!(month["assets"][0]["aspect"], 1.5);
    let (status, _) = rig
        .json(rig.req(reqwest::Method::GET, "/library/timeline/2025-1", ALICE))
        .await;
    assert_eq!(status, 400);

    let (_, albums) = rig
        .json(rig.req(reqwest::Method::GET, "/library/albums", ALICE))
        .await;
    let album = &albums["albums"][0];
    assert_eq!(
        (album["owner"].clone(), album["shared"].clone()),
        (json!("household"), json!(true))
    );
    assert_eq!(album["members"], json!(["alice", "dad"]));
    let _ = alice_id;
}

#[tokio::test]
async fn bytes_pass_through_with_range_and_unknown_renditions_are_refused() {
    let rig = rig().await;
    let (_, done) = rig.upload(ALICE, "c.jpg", b"pixels", None).await;
    let id = done["item_id"].as_str().unwrap();
    let path = format!("/library/items/{id}/content?rendition=thumbnail");
    let response = rig
        .req(reqwest::Method::GET, &path, ALICE)
        .header("Range", "bytes=0-3")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 206);
    assert!(
        response.headers()["content-range"]
            .to_str()
            .unwrap()
            .starts_with("bytes 0-3/")
    );
    assert_eq!(response.text().await.unwrap(), "thum");

    let cached = rig
        .req(reqwest::Method::GET, &path, ALICE)
        .header("If-None-Match", format!("\"{id}\""))
        .send()
        .await
        .unwrap();
    assert_eq!(cached.status().as_u16(), 304, "缓存命中照原样回，不是失败");
    let beyond = rig
        .req(reqwest::Method::GET, &path, ALICE)
        .header("Range", "bytes=999-")
        .send()
        .await
        .unwrap();
    assert_eq!(beyond.status().as_u16(), 416);
    assert!(beyond.headers().contains_key("content-range"));

    let bad = format!("/library/items/{id}/content?rendition=hologram");
    assert_eq!(
        rig.json(rig.req(reqwest::Method::GET, &bad, ALICE)).await.1["code"],
        "protocol"
    );
    let traversal = "/library/items/..%2F..%2Fadmin%2Fusers/content?rendition=original";
    assert_eq!(
        rig.json(rig.req(reqwest::Method::GET, traversal, ALICE))
            .await
            .1["code"],
        "media_gone"
    );
}

#[tokio::test]
async fn native_actions_run_as_the_person_and_create_shared_albums() {
    let rig = rig().await;
    let (_, done) = rig.upload(ALICE, "d.jpg", b"dinner", None).await;
    rig.json(rig.req(reqwest::Method::GET, "/library/timeline", OWNER))
        .await;
    let id = done["item_id"].clone();

    let fav = json!({"kind": "favorite", "ids": [id], "favorite": true});
    let (status, result) = rig
        .json(
            rig.req(reqwest::Method::POST, "/library/actions", ALICE)
                .json(&fav),
        )
        .await;
    assert_eq!((status, result), (200, json!({"changed": 1})));
    assert!(rig.world.lock().unwrap().assets[0].favorite);

    let (_, features) = rig
        .json(rig.req(reqwest::Method::GET, "/library", ALICE))
        .await;
    assert!(
        features["actions"]
            .as_array()
            .unwrap()
            .contains(&json!("favorite"))
    );

    let create =
        json!({"kind": "create_album", "title": "晚饭", "ids": [id], "share_with": ["dad"]});
    let (status, result) = rig
        .json(
            rig.req(reqwest::Method::POST, "/library/actions", ALICE)
                .json(&create),
        )
        .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(
        (
            result["album"]["owner"].clone(),
            result["album"]["members"].clone()
        ),
        (json!("me"), json!(["dad"]))
    );

    let stranger =
        json!({"kind": "share_album", "album": result["album"]["id"], "people": ["grandma"]});
    let (status, refused) = rig
        .json(
            rig.req(reqwest::Method::POST, "/library/actions", ALICE)
                .json(&stranger),
        )
        .await;
    assert_eq!(
        (status, refused["code"].clone()),
        (403, json!("permission_denied"))
    );
    let unknown = json!({"kind": "rotate", "ids": [id]});
    let (status, refused) = rig
        .json(
            rig.req(reqwest::Method::POST, "/library/actions", ALICE)
                .json(&unknown),
        )
        .await;
    assert_eq!(
        (status, refused["code"].clone()),
        (400, json!("protocol")),
        "不认识的动作不静默成功"
    );
}

#[tokio::test]
async fn a_key_revoked_in_immich_is_reported_then_reopened_on_the_next_request() {
    let rig = rig().await;
    rig.upload(ALICE, "e.jpg", b"e", None).await;
    rig.world.lock().unwrap().keys.clear();
    let (status, failed) = rig
        .json(rig.req(reqwest::Method::GET, "/library/timeline", ALICE))
        .await;
    assert_eq!(
        (status, failed["code"].clone()),
        (401, json!("authentication_required"))
    );
    let (status, _) = rig
        .json(rig.req(reqwest::Method::GET, "/library/timeline", ALICE))
        .await;
    assert_eq!(status, 200);
    let world = rig.world.lock().unwrap();
    assert_eq!(
        world
            .users
            .iter()
            .filter(|u| u.email == "alice@wonton.internal")
            .count(),
        1,
        "重开不重建号"
    );
    assert!(world.sessions.is_empty(), "开户用的会话都退出了");
}

#[tokio::test]
async fn a_batch_with_someone_elses_photo_does_the_rest_and_lists_the_refused_one() {
    let rig = rig().await;
    let (_, done) = rig.upload(ALICE, "e.jpg", b"evening", None).await;
    let mine = done["item_id"].clone();
    let theirs = {
        let mut world = rig.world.lock().unwrap();
        let dad = world.user_by_email("dad@example.com").unwrap().id.clone();
        world.add_asset(&dad, "2025-11-03T10:00:00.000Z")
    };
    let fav = json!({"kind": "favorite", "ids": [mine, theirs], "favorite": true});
    let (status, result) = rig
        .json(
            rig.req(reqwest::Method::POST, "/library/actions", ALICE)
                .json(&fav),
        )
        .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["changed"], 1);
    assert_eq!(result["failed"][0]["id"], json!(theirs));
    assert_eq!(result["failed"][0]["failure"]["code"], "permission_denied");
    {
        let world = rig.world.lock().unwrap();
        assert!(
            world
                .assets
                .iter()
                .find(|a| json!(a.id) == mine)
                .unwrap()
                .favorite
        );
        assert!(
            !world
                .assets
                .iter()
                .find(|a| json!(a.id) == json!(theirs))
                .unwrap()
                .favorite
        );
    }

    let only_theirs = json!({"kind": "favorite", "ids": [theirs], "favorite": true});
    let (status, refused) = rig
        .json(
            rig.req(reqwest::Method::POST, "/library/actions", ALICE)
                .json(&only_theirs),
        )
        .await;
    assert_eq!(
        (status, refused["code"].clone()),
        (403, json!("permission_denied")),
        "别人的照片是没权限，不是请求写错了"
    );
}
