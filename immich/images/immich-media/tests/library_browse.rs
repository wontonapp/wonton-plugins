//! 浏览侧的路由：搜索、相册详情、人物。假 Immich 回什么，适配器就得按契约翻什么、归什么类。

mod common;
mod fake_immich;

use serde_json::{Value, json};

use common::{ALICE, UNKNOWN, rig};
use fake_immich::World;

fn rename(world: &mut World, id: &str, name: &str) {
    world.asset(id).name = name.into();
}

#[tokio::test]
async fn search_maps_hits_and_the_contract_query_to_immich() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let (mine, mate) = {
        let mut world = rig.world.lock().unwrap();
        let mine = world.add_asset(&alice, "2025-11-03T10:00:00.000Z");
        rename(&mut world, &mine, "sea.jpg");
        let asset = world.asset(&mine);
        asset.city = Some("Qingdao".into());
        asset.country = Some("China".into());
        let mate = world.add_person(&alice, "grandma");
        world.asset(&mine).person = Some(mate.clone());
        (mine, mate)
    };

    let (status, page) = rig
        .json(rig.req(reqwest::Method::GET, "/library/search?q=sea", ALICE))
        .await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(
        page["assets"],
        json!([{
            "id": mine, "kind": "photo", "taken_at": 1_762_164_000_000u64,
            "aspect": 1.5, "live": false, "favorite": false,
        }]),
        "自己的照片不带 owner"
    );
    assert_eq!(
        rig.world.lock().unwrap().searches,
        vec![("smart", json!({"page": 1, "size": 200, "query": "sea"}))],
        "有画面描述走 smart，条件名照 Immich 的"
    );

    let (_, filtered) = rig
        .json(rig.req(
            reqwest::Method::GET,
            &format!(
                "/library/search?people={mate}&from=1762164000000&to=1762250400000&kind=video&favorite=true"
            ),
            ALICE,
        ))
        .await;
    assert_eq!(filtered, json!({"assets": []}));
    let searches = rig.world.lock().unwrap().searches.clone();
    assert_eq!(
        searches[1],
        (
            "metadata",
            json!({
                "page": 1, "size": 200, "personIds": [mate],
                "takenAfter": "2025-11-03T10:00:00.000Z",
                "takenBefore": "2025-11-04T10:00:00.000Z",
                "type": "VIDEO", "isFavorite": true,
            })
        ),
        "没画面描述走 metadata；人物、时间、种类、收藏都照 Immich 的字段"
    );
}

#[tokio::test]
async fn search_tries_the_city_then_the_country_and_an_unknown_place_is_an_empty_page() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let seen = {
        let mut world = rig.world.lock().unwrap();
        let seen = world.add_asset(&alice, "2025-11-03T10:00:00.000Z");
        let asset = world.asset(&seen);
        asset.city = Some("Qingdao".into());
        asset.country = Some("China".into());
        seen
    };
    let (status, nobody) = rig
        .json(rig.req(
            reqwest::Method::GET,
            &format!("/library/search?people={UNKNOWN}"),
            ALICE,
        ))
        .await;
    assert_eq!(
        (status, nobody["assets"].clone()),
        (200, json!([])),
        "没建过的人 Immich 也回 200 空页，不是 404"
    );

    let (status, city) = rig
        .json(rig.req(reqwest::Method::GET, "/library/search?place=Qingdao", ALICE))
        .await;
    assert_eq!(status, 200, "{city}");
    assert_eq!(city["assets"][0]["id"], json!(seen));

    // 城市没有就再按国家搜一次；两个都没有就回空页，不是错误。
    let (status, empty) = rig
        .json(rig.req(
            reqwest::Method::GET,
            "/library/search?place=Atlantis",
            ALICE,
        ))
        .await;
    assert_eq!((status, empty["assets"].clone()), (200, json!([])));
    let places: Vec<Value> = rig
        .world
        .lock()
        .unwrap()
        .searches
        .iter()
        .map(|(_, body)| json!({"city": body["city"], "country": body["country"]}))
        .collect();
    assert_eq!(
        places,
        vec![
            json!({"city": null, "country": null}),
            json!({"city": "Qingdao", "country": null}),
            json!({"city": "Atlantis", "country": null}),
            json!({"city": null, "country": "Atlantis"}),
        ]
    );
}

#[tokio::test]
async fn search_shows_partner_work_with_its_owner_and_pages_with_cursors() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let dad = rig.dad();
    let (shared, hidden) = {
        let mut world = rig.world.lock().unwrap();
        let shared = world.add_asset(&dad, "2025-11-06T10:00:00.000Z");
        rename(&mut world, &shared, "sea-shared.jpg");
        let hidden = world.add_asset(&dad, "2025-11-07T10:00:00.000Z");
        rename(&mut world, &hidden, "hidden.jpg");
        (shared, hidden)
    };
    let (status, none) = rig
        .json(rig.req(reqwest::Method::GET, "/library/search?q=hidden", ALICE))
        .await;
    assert_eq!(
        (status, none["assets"].clone()),
        (200, json!([])),
        "没共享的 partner 照片看不到"
    );

    rig.world
        .lock()
        .unwrap()
        .partners
        .push((dad.clone(), alice.clone()));
    let (_, page) = rig
        .json(rig.req(reqwest::Method::GET, "/library/search?q=hidden", ALICE))
        .await;
    assert_eq!(page["assets"][0]["id"], json!(hidden), "共享后看得到");

    {
        let mut world = rig.world.lock().unwrap();
        for _ in 0..201 {
            world.add_asset(&alice, "2025-11-03T10:00:00.000Z");
        }
    }
    let (status, page) = rig
        .json(rig.req(reqwest::Method::GET, "/library/search?q=sea", ALICE))
        .await;
    assert_eq!(status, 200);
    assert_eq!(page["assets"].as_array().unwrap().len(), 1);
    assert_eq!(page["assets"][0]["id"], json!(shared));
    assert_eq!(
        page["assets"][0]["owner"],
        json!("dad"),
        "partner 给我的那张带库主"
    );
    assert_eq!(page["cursor"], Value::Null);

    let (status, first) = rig
        .json(rig.req(reqwest::Method::GET, "/library/search?q=0000", ALICE))
        .await;
    assert_eq!(status, 200);
    assert_eq!(first["assets"].as_array().unwrap().len(), 200);
    let cursor = first["cursor"].as_str().unwrap().to_owned();
    assert_eq!(cursor, "2");

    let (status, second) = rig
        .json(rig.req(
            reqwest::Method::GET,
            &format!("/library/search?q=0000&cursor={cursor}"),
            ALICE,
        ))
        .await;
    assert_eq!(status, 200);
    assert_eq!(second["assets"].as_array().unwrap().len(), 1);
    assert_eq!(second["cursor"], Value::Null, "最后一页不再给游标");
}

#[tokio::test]
async fn album_page_carries_the_album_its_items_and_the_next_page() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let dad = rig.dad();
    let (album, theirs, mine) = {
        let mut world = rig.world.lock().unwrap();
        let family = world.add_user("family@wonton.internal", "x");
        let album = world.add_album("家庭", &family, &[&alice, &dad]);
        let mine = world.add_asset(&alice, "2025-11-03T10:00:00.000Z");
        let theirs = world.add_asset(&dad, "2025-11-04T10:00:00.000Z");
        let album_mut = world.albums.iter_mut().find(|a| a.0 == album).unwrap();
        album_mut.3 = vec![mine.clone(), theirs.clone()];
        (album, theirs, mine)
    };

    let (status, page) = rig
        .json(rig.req(
            reqwest::Method::GET,
            &format!("/library/albums/{album}"),
            ALICE,
        ))
        .await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(
        page["album"],
        json!({
            "id": album, "title": "家庭", "count": 2, "owner": "household",
            "shared": true, "cover": mine, "members": ["alice", "dad"],
            "updated_at": 1_795_161_600_000u64,
        })
    );
    let items = page["assets"].as_array().unwrap();
    assert_eq!(
        items.iter().map(|a| a["id"].clone()).collect::<Vec<_>>(),
        vec![json!(theirs), json!(mine)],
        "相册里新的在前"
    );
    assert_eq!(items[0]["owner"], json!("dad"));
    assert_eq!(items[1]["owner"], Value::Null);
    let searches = rig.world.lock().unwrap().searches.clone();
    assert_eq!(
        searches[0],
        (
            "metadata",
            json!({"albumIds": [album], "page": 1, "size": 200, "order": "desc"})
        )
    );

    let (status, second) = rig
        .json(rig.req(
            reqwest::Method::GET,
            &format!("/library/albums/{album}?cursor=2"),
            ALICE,
        ))
        .await;
    assert_eq!((status, second["assets"].clone()), (200, json!([])));
}

#[tokio::test]
async fn an_album_that_is_gone_is_media_gone_and_a_strangers_album_is_permission_denied() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let dad = rig.dad();
    let (vanished, strangers) = {
        let mut world = rig.world.lock().unwrap();
        let vanished = world.add_album("旧相册", &dad, &[&alice]);
        world.vanish_album(&vanished);
        let strangers = world.add_album("他的相册", &dad, &[]);
        (vanished, strangers)
    };

    // 行在权限检查之后才被删的竞态窗口；行早没了会先在 access 回 400。
    let path = format!("/library/albums/{vanished}");
    let (status, gone) = rig.json(rig.req(reqwest::Method::GET, &path, ALICE)).await;
    assert_eq!((status, gone["code"].clone()), (404, json!("media_gone")));

    let path = format!("/library/albums/{strangers}");
    let (status, refused) = rig.json(rig.req(reqwest::Method::GET, &path, ALICE)).await;
    assert_eq!(
        (status, refused["code"].clone()),
        (403, json!("permission_denied")),
        "Immich 的 400 Not found or no album.read 是没权限，不是请求写错"
    );
    assert!(
        refused["message"]
            .as_str()
            .unwrap()
            .contains("Not found or no album.read access")
    );
    let (status, gone) = rig
        .json(rig.req(
            reqwest::Method::GET,
            &format!("/library/albums/{UNKNOWN}"),
            ALICE,
        ))
        .await;
    assert_eq!(
        (status, gone["code"].clone()),
        (403, json!("permission_denied"))
    );
}

#[tokio::test]
async fn people_lists_named_people_only_and_their_thumbnails_pass_through() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let dad = rig.dad();
    let (mine, theirs) = {
        let mut world = rig.world.lock().unwrap();
        world.partners.push((dad.clone(), alice.clone()));
        let mine = world.add_person(&alice, "grandma");
        world.add_person(&alice, "");
        let theirs = world.add_person(&dad, "grandpa");
        (mine, theirs)
    };

    let (status, list) = rig
        .json(rig.req(reqwest::Method::GET, "/library/people", ALICE))
        .await;
    assert_eq!(status, 200, "{list}");
    assert_eq!(
        list,
        json!({"people": [
            {"id": mine, "name": "grandma"},
            {"id": theirs, "name": "grandpa"},
        ]}),
        "没名字的不列，partner 的人列"
    );

    let response = rig
        .req(
            reqwest::Method::GET,
            &format!("/library/people/{mine}/thumbnail"),
            ALICE,
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.headers()["content-type"], "image/jpeg");
    assert_eq!(response.text().await.unwrap(), format!("face-of-{mine}"));
}

#[tokio::test]
async fn a_person_whose_file_is_gone_is_media_gone_and_a_strangers_person_is_permission_denied() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let dad = rig.dad();
    let (missing, strangers) = {
        let mut world = rig.world.lock().unwrap();
        let missing = world.add_person(&alice, "grandma");
        world
            .people
            .iter_mut()
            .find(|p| p.id == missing)
            .unwrap()
            .file_missing = true;
        let strangers = world.add_person(&dad, "grandpa");
        (missing, strangers)
    };

    let path = format!("/library/people/{missing}/thumbnail");
    let (status, gone) = rig.json(rig.req(reqwest::Method::GET, &path, ALICE)).await;
    assert_eq!((status, gone["code"].clone()), (404, json!("media_gone")));

    let path = format!("/library/people/{strangers}/thumbnail");
    let (status, refused) = rig.json(rig.req(reqwest::Method::GET, &path, ALICE)).await;
    assert_eq!(
        (status, refused["code"].clone()),
        (403, json!("permission_denied"))
    );
    let path = format!("/library/people/{UNKNOWN}/thumbnail");
    let (status, refused) = rig.json(rig.req(reqwest::Method::GET, &path, ALICE)).await;
    assert_eq!(
        (status, refused["code"].clone()),
        (403, json!("permission_denied"))
    );
}

#[tokio::test]
async fn item_detail_maps_exif_place_people_owner_and_falls_back_without_exif() {
    let rig = rig().await;
    let alice = rig.alice().await;
    let dad = rig.dad();
    let (mine, bare, mate, shared, stranger) = {
        let mut world = rig.world.lock().unwrap();
        let mine = world.add_asset(&alice, "2025-11-03T10:00:00.000Z");
        world.asset(&mine).name = "IMG_9.HEIC".into();
        world.asset(&mine).exif = Some(json!({
            "make": "Apple", "model": "iPhone 15 Pro", "lensModel": "Main Camera",
            "description": "海边", "city": "Qingdao", "state": "山东", "country": "China",
            "latitude": 36.06, "longitude": 120.38,
            "exifImageWidth": 4032, "exifImageHeight": 3024, "fileSizeInByte": 4321,
        }));
        let mate = world.add_person(&alice, "grandma");
        world.add_person(&alice, "");
        world.asset(&mine).person = Some(mate.clone());

        // 没有 EXIF 的一条，带一个没名字的人：认不出的人不列。
        let bare = world.add_asset(&alice, "2025-11-04T10:00:00.000Z");
        let unnamed = world.add_person(&alice, "");
        world.asset(&bare).person = Some(unnamed);

        world.partners.push((dad.clone(), alice.clone()));
        let shared = world.add_asset(&dad, "2025-11-05T10:00:00.000Z");
        world.asset(&shared).exif = Some(json!({"make": "Apple", "model": "Apple iPhone 15"}));

        let grandma = world.add_user("grandma@example.com", "x");
        let stranger = world.add_asset(&grandma, "2025-11-06T10:00:00.000Z");
        (mine, bare, mate, shared, stranger)
    };

    let path = format!("/library/items/{mine}");
    let (status, detail) = rig.json(rig.req(reqwest::Method::GET, &path, ALICE)).await;
    assert_eq!(status, 200, "{detail}");
    assert_eq!(
        detail,
        json!({
            "asset": {
                "id": mine, "kind": "photo", "taken_at": 1_762_164_000_000u64,
                "aspect": 1.5, "live": false, "favorite": false,
            },
            "width": 4032, "height": 3024, "file_name": "IMG_9.HEIC", "size": 4321,
            "description": "海边", "camera": "Apple iPhone 15 Pro", "lens": "Main Camera",
            "place": ["Qingdao", "山东", "China"],
            "location": {"lat": 36.06, "lon": 120.38},
            "people": [{"id": mate, "name": "grandma"}],
        })
    );

    // 没有 EXIF：尺寸、文件名用条目本身的；大小、描述、相机、镜头、地点、人都不编。
    let path = format!("/library/items/{bare}");
    let (status, plain) = rig.json(rig.req(reqwest::Method::GET, &path, ALICE)).await;
    assert_eq!(status, 200, "{plain}");
    assert_eq!(
        plain,
        json!({
            "asset": {
                "id": bare, "kind": "photo", "taken_at": 1_762_250_400_000u64,
                "aspect": 1.5, "live": false, "favorite": false,
            },
            "width": 3000, "height": 2000,
            "file_name": format!("{bare}.jpg"),
        })
    );

    let path = format!("/library/items/{shared}");
    let (status, partner) = rig.json(rig.req(reqwest::Method::GET, &path, ALICE)).await;
    assert_eq!(status, 200, "{partner}");
    assert_eq!(partner["asset"]["owner"], json!("dad"));
    assert_eq!(
        partner["camera"],
        json!("Apple iPhone 15"),
        "型号已经带厂牌就不再拼"
    );

    for id in [stranger, UNKNOWN.to_owned()] {
        let path = format!("/library/items/{id}");
        let (status, refused) = rig.json(rig.req(reqwest::Method::GET, &path, ALICE)).await;
        assert_eq!(
            (status, refused["code"].clone()),
            (403, json!("permission_denied"))
        );
        assert!(
            refused["message"]
                .as_str()
                .unwrap()
                .contains("Not found or no asset.read access")
        );
    }
}
