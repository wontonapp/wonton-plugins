//! 取字节：原图、视频播放（`Range`）、实况的动态部分。假 Immich 的字节、头和状态都原样过。

mod common;
mod fake_immich;

use serde_json::json;

use common::{ALICE, UNKNOWN, rig};

fn content(id: &str, rendition: &str) -> String {
    format!("/library/items/{id}/content?rendition={rendition}")
}

#[tokio::test]
async fn original_passes_the_bytes_and_headers_through() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let id = {
        let mut world = rig.world.lock().unwrap();
        world.add_asset(&alice, "2025-11-03T10:00:00.000Z")
    };
    let response = rig
        .req(reqwest::Method::GET, &content(&id, "original"), ALICE)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.headers()["content-type"], "image/jpeg");
    assert_eq!(response.headers()["accept-ranges"], "bytes");
    let body = format!("original-of-{id}");
    assert_eq!(
        response.headers()["content-length"].to_str().unwrap(),
        body.len().to_string()
    );
    assert_eq!(response.text().await.unwrap(), body);
}

#[tokio::test]
async fn original_of_a_row_without_a_file_is_media_gone_and_a_strangers_is_permission_denied() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let dad = rig.dad();
    let (missing, strangers) = {
        let mut world = rig.world.lock().unwrap();
        let missing = world.add_asset(&alice, "2025-11-03T10:00:00.000Z");
        world.remove_asset_file(&missing);
        let strangers = world.add_asset(&dad, "2025-11-04T10:00:00.000Z");
        (missing, strangers)
    };

    let (status, gone) = rig
        .json(rig.req(reqwest::Method::GET, &content(&missing, "original"), ALICE))
        .await;
    assert_eq!((status, gone["code"].clone()), (404, json!("media_gone")));

    let (status, refused) = rig
        .json(rig.req(
            reqwest::Method::GET,
            &content(&strangers, "original"),
            ALICE,
        ))
        .await;
    assert_eq!(
        (status, refused["code"].clone()),
        (403, json!("permission_denied")),
        "Immich 的 400 是没权限，不是请求写错"
    );
    assert!(
        refused["message"]
            .as_str()
            .unwrap()
            .contains("Not found or no asset.download access")
    );
    let (status, refused) = rig
        .json(rig.req(reqwest::Method::GET, &content(UNKNOWN, "original"), ALICE))
        .await;
    assert_eq!(
        (status, refused["code"].clone()),
        (403, json!("permission_denied"))
    );
}

#[tokio::test]
async fn video_playback_passes_range_requests_and_the_partial_reply() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let id = {
        let mut world = rig.world.lock().unwrap();
        world.add_video(&alice, "2025-11-03T10:00:00.000Z")
    };
    let path = content(&id, "playback");
    let ranged = rig
        .req(reqwest::Method::GET, &path, ALICE)
        .header("Range", "bytes=4-7")
        .send()
        .await
        .unwrap();
    assert_eq!(ranged.status().as_u16(), 206);
    assert!(
        ranged.headers()["content-range"]
            .to_str()
            .unwrap()
            .starts_with("bytes 0-3/")
    );
    assert_eq!(
        rig.world.lock().unwrap().ranges,
        vec!["bytes=4-7".to_owned()],
        "`Range` 原样到了 Immich"
    );
    assert_eq!(ranged.headers()["content-type"], "video/mp4");
    assert_eq!(ranged.text().await.unwrap(), "vide");

    let whole = rig
        .req(reqwest::Method::GET, &path, ALICE)
        .send()
        .await
        .unwrap();
    assert_eq!(whole.status().as_u16(), 200);
    assert_eq!(whole.text().await.unwrap(), format!("video-bytes-of-{id}"));
}

#[tokio::test]
async fn playback_of_a_photo_or_a_missing_file_is_media_gone_and_a_strangers_video_is_permission_denied()
 {
    let rig = rig().await;
    let alice = rig.alice().await;
    let dad = rig.dad();
    let (photo, without_file, strangers) = {
        let mut world = rig.world.lock().unwrap();
        let photo = world.add_asset(&alice, "2025-11-03T10:00:00.000Z");
        let without_file = world.add_video(&alice, "2025-11-04T10:00:00.000Z");
        world.remove_asset_file(&without_file);
        let strangers = world.add_video(&dad, "2025-11-05T10:00:00.000Z");
        (photo, without_file, strangers)
    };

    for id in [photo, without_file] {
        let (status, gone) = rig
            .json(rig.req(reqwest::Method::GET, &content(&id, "playback"), ALICE))
            .await;
        assert_eq!(status, 404, "{gone}");
        assert_eq!(gone["code"], json!("media_gone"));
        assert!(
            gone["message"]
                .as_str()
                .unwrap()
                .contains("Asset not found or asset is not a video")
        );
    }
    let (status, refused) = rig
        .json(rig.req(reqwest::Method::GET, &content(UNKNOWN, "playback"), ALICE))
        .await;
    assert_eq!(
        (status, refused["code"].clone()),
        (403, json!("permission_denied")),
        "没建过的编号在 Immich 是 400"
    );

    let (status, refused) = rig
        .json(rig.req(
            reqwest::Method::GET,
            &content(&strangers, "playback"),
            ALICE,
        ))
        .await;
    assert_eq!(
        (status, refused["code"].clone()),
        (403, json!("permission_denied"))
    );
}

#[tokio::test]
async fn live_photo_plays_the_motion_part_and_passes_range_on() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let (still, video) = {
        let mut world = rig.world.lock().unwrap();
        let video = world.add_video(&alice, "2025-11-03T10:00:00.000Z");
        let still = world.add_asset(&alice, "2025-11-03T10:00:01.000Z");
        world.asset(&still).live = Some(video.clone());
        (still, video)
    };
    let response = rig
        .req(reqwest::Method::GET, &content(&still, "live"), ALICE)
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
    assert_eq!(
        rig.world.lock().unwrap().ranges,
        vec!["bytes=0-3".to_owned()]
    );
    assert_eq!(response.headers()["content-type"], "video/mp4");
    assert_eq!(response.text().await.unwrap(), "vide");
    let _ = video;
}

#[tokio::test]
async fn live_on_a_still_without_motion_or_a_video_that_is_gone_is_media_gone() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let (plain, hollow) = {
        let mut world = rig.world.lock().unwrap();
        let plain = world.add_asset(&alice, "2025-11-03T10:00:00.000Z");
        let hollow = world.add_asset(&alice, "2025-11-04T10:00:00.000Z");
        let video = world.add_video(&alice, "2025-11-04T10:00:01.000Z");
        world.remove_asset_file(&video);
        world.asset(&hollow).live = Some(video);
        (plain, hollow)
    };

    for id in [plain.clone(), hollow] {
        let (status, gone) = rig
            .json(rig.req(reqwest::Method::GET, &content(&id, "live"), ALICE))
            .await;
        assert_eq!(status, 404, "{gone}");
        assert_eq!(gone["code"], json!("media_gone"));
    }
    let (_, refused) = rig
        .json(rig.req(reqwest::Method::GET, &content(UNKNOWN, "live"), ALICE))
        .await;
    assert_eq!(refused["code"], json!("permission_denied"));
    let (_, not_live) = rig
        .json(rig.req(reqwest::Method::GET, &content(&plain, "live"), ALICE))
        .await;
    assert_eq!(not_live["message"], json!("这一条不是实况照片"));
}

#[tokio::test]
async fn live_of_a_strangers_still_is_permission_denied() {
    let rig = rig().await;
    let dad = rig.dad();
    let id = {
        let mut world = rig.world.lock().unwrap();
        let video = world.add_video(&dad, "2025-11-03T10:00:00.000Z");
        let still = world.add_asset(&dad, "2025-11-03T10:00:01.000Z");
        world.asset(&still).live = Some(video);
        still
    };
    let (status, refused) = rig
        .json(rig.req(reqwest::Method::GET, &content(&id, "live"), ALICE))
        .await;
    assert_eq!(
        (status, refused["code"].clone()),
        (403, json!("permission_denied"))
    );
}
