//! 从回收站恢复：Immich 这条是全有或全无、只认主人，适配器先整批、被拒再逐条，做不了的进 `failed`。

mod common;
mod fake_immich;

use serde_json::json;

use common::{ALICE, UNKNOWN, rig};

fn restore_body(ids: &[String]) -> serde_json::Value {
    json!({"kind": "restore", "ids": ids})
}

#[tokio::test]
async fn restore_puts_the_photo_back_in_the_library() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let id = {
        let mut world = rig.world.lock().unwrap();
        world.add_trashed_asset(&alice, "2025-11-03T10:00:00.000Z")
    };

    let (status, result) = rig
        .json(
            rig.req(reqwest::Method::POST, "/library/actions", ALICE)
                .json(&restore_body(std::slice::from_ref(&id))),
        )
        .await;
    assert_eq!((status, result), (200, json!({"changed": 1})));
    let world = rig.world.lock().unwrap();
    assert!(!world.assets[0].trashed);
    assert_eq!(world.restores, vec![json!({"ids": [id]})]);
}

#[tokio::test]
async fn restore_of_an_id_that_is_gone_or_someone_elses_is_permission_denied() {
    let rig = rig().await;
    let dad = rig.dad();
    let theirs = {
        let mut world = rig.world.lock().unwrap();
        world.add_trashed_asset(&dad, "2025-11-03T10:00:00.000Z")
    };

    for id in [UNKNOWN.to_owned(), theirs.clone()] {
        let (status, refused) = rig
            .json(
                rig.req(reqwest::Method::POST, "/library/actions", ALICE)
                    .json(&restore_body(&[id])),
            )
            .await;
        assert_eq!(
            (status, refused["code"].clone()),
            (403, json!("permission_denied")),
            "Immich 对不在的、别人的都回 400 Not found or no asset.delete access"
        );
        assert!(
            refused["message"]
                .as_str()
                .unwrap()
                .contains("Not found or no asset.delete access")
        );
    }
    assert!(rig.world.lock().unwrap().assets[0].trashed);
}

#[tokio::test]
async fn restore_in_a_batch_brings_back_the_owners_photo_and_lists_the_refused_ones() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let dad = rig.dad();
    let (mine, theirs) = {
        let mut world = rig.world.lock().unwrap();
        let mine = world.add_trashed_asset(&alice, "2025-11-03T10:00:00.000Z");
        let theirs = world.add_trashed_asset(&dad, "2025-11-04T10:00:00.000Z");
        (mine, theirs)
    };
    let ids = vec![mine.clone(), theirs.clone(), UNKNOWN.to_owned()];

    let (status, result) = rig
        .json(
            rig.req(reqwest::Method::POST, "/library/actions", ALICE)
                .json(&restore_body(&ids)),
        )
        .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["changed"], 1);
    assert_eq!(
        result["failed"],
        json!([
            {"id": theirs, "failure": {"code": "permission_denied", "message": "Immich POST /trash/restore/assets 返回 400 Bad Request：Not found or no asset.delete access"}},
            {"id": UNKNOWN, "failure": {"code": "permission_denied", "message": "Immich POST /trash/restore/assets 返回 400 Bad Request：Not found or no asset.delete access"}},
        ])
    );
    let world = rig.world.lock().unwrap();
    assert!(!world.assets[0].trashed, "自己的那张恢复了");
    assert!(world.assets[1].trashed, "别人的那张没动");
    assert_eq!(world.restores.len(), 4, "先整批，再逐条：整批 + 三条");
}
