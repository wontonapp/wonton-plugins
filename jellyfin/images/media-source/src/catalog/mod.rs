//! 影视浏览（规范“媒体源：影视浏览”，操作 `catalog`）：片库、首页几排、海报墙、详情、分类和本人的
//! 已看、收藏。全部以这个人自己的 Jellyfin 令牌调，进度、已看、家长分级都是他的。

pub mod map;
pub mod wire;

use reqwest::Method;
use serde::Deserialize;

use crate::accounts::Account;
use crate::failure::{Failure, Reply};
use crate::jellyfin::{Jellyfin, encode};
use map::{DETAIL_FIELDS, JfEntry, LIST_FIELDS};
use wire::{
    ActionFailure, CatalogAction, CatalogActionResult, EntryDetail, EntryPage, Genres, Libraries,
};

const PAGE: usize = 60;
/// 首页一排最多几项；一排不翻页。
const SHELF: usize = 24;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JfPage {
    items: Vec<JfEntry>,
    #[serde(default)]
    total_record_count: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
pub struct EntriesQuery {
    pub library: Option<String>,
    pub parent: Option<String>,
    pub entry_type: Option<String>,
    pub genre: Option<String>,
    pub q: Option<String>,
    pub sort: Option<String>,
    pub unplayed: Option<bool>,
    pub cursor: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LibraryQuery {
    pub library: Option<String>,
}

fn user(who: &Account) -> String {
    encode(&who.user_id)
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|v| !v.is_empty())
}

pub async fn libraries(jf: &Jellyfin, who: &Account) -> Reply<Libraries> {
    let path = format!("/UserViews?userId={}", user(who));
    let page: JfPage = jf.get(&path, &who.identity(), "列片库").await?;
    Ok(Libraries {
        libraries: page.items.iter().filter_map(map::library).collect(),
    })
}

/// 排序翻成 Jellyfin 的 `SortBy`/`SortOrder`；列某个父项的孩子时默认按季号、集号排。
fn sort_clause(sort: Option<&str>, has_parent: bool) -> Reply<&'static str> {
    Ok(match sort {
        None if has_parent => "SortBy=ParentIndexNumber,IndexNumber,SortName&SortOrder=Ascending",
        None | Some("title") => "SortBy=SortName&SortOrder=Ascending",
        Some("added") => "SortBy=DateCreated,SortName&SortOrder=Descending",
        Some("released") => "SortBy=PremiereDate,ProductionYear,SortName&SortOrder=Descending",
        Some("rating") => "SortBy=CommunityRating,SortName&SortOrder=Descending",
        Some(other) => return Err(Failure::bad_request(format!("不认识的排序：{other}"))),
    })
}

/// 海报墙要的 Jellyfin 查询串（不含 `userId`）。纯函数，便于对照规范测。
pub fn entries_query(q: &EntriesQuery) -> Reply<String> {
    let start = crate::items::offset(q.cursor.as_deref())?;
    let parent = non_empty(&q.parent).or(non_empty(&q.library));
    let searching = non_empty(&q.q).is_some();
    let types = match non_empty(&q.entry_type) {
        Some(t) => Some(
            map::jellyfin_type(t)
                .ok_or_else(|| Failure::bad_request(format!("不认识的条目层级：{t}")))?,
        ),
        // 只列父项的直接孩子（剧 → 季，季 → 集）；片库最上一层列电影、剧和单个视频。
        None if non_empty(&q.parent).is_some() && !searching => None,
        None => Some("Movie,Series,Video"),
    };
    let recursive = types.is_some() || searching;
    let mut out = format!(
        "Recursive={recursive}&{}&StartIndex={start}&Limit={PAGE}&EnableTotalRecordCount=true&{LIST_FIELDS}",
        sort_clause(non_empty(&q.sort), non_empty(&q.parent).is_some())?
    );
    if let Some(types) = types {
        out.push_str(&format!("&IncludeItemTypes={types}"));
    }
    if let Some(parent) = parent {
        out.push_str(&format!("&ParentId={}", encode(parent)));
    }
    if let Some(genre) = non_empty(&q.genre) {
        out.push_str(&format!("&Genres={}", encode(genre)));
    }
    if let Some(term) = non_empty(&q.q) {
        out.push_str(&format!("&SearchTerm={}", encode(term)));
    }
    if q.unplayed == Some(true) {
        out.push_str("&IsPlayed=false");
    }
    Ok(out)
}

pub async fn entries(jf: &Jellyfin, who: &Account, q: &EntriesQuery) -> Reply<EntryPage> {
    let start = crate::items::offset(q.cursor.as_deref())?;
    let path = format!("/Items?userId={}&{}", user(who), entries_query(q)?);
    let page: JfPage = jf.get(&path, &who.identity(), "列海报墙").await?;
    let next = start + page.items.len();
    let total = page.total_record_count;
    Ok(EntryPage {
        entries: page.items.iter().map(map::entry).collect(),
        cursor: (!page.items.is_empty() && total.is_some_and(|t| next < t as usize))
            .then(|| next.to_string()),
        total,
    })
}

pub async fn shelf(
    jf: &Jellyfin,
    who: &Account,
    shelf: &str,
    library: Option<&str>,
) -> Reply<EntryPage> {
    let parent = library
        .filter(|l| !l.is_empty())
        .map(|l| format!("&parentId={}", encode(l)))
        .unwrap_or_default();
    let u = user(who);
    let id = who.identity();
    let items = match shelf {
        "resume" => {
            let path = format!(
                "/UserItems/Resume?userId={u}&mediaTypes=Video&limit={SHELF}{parent}&{LIST_FIELDS}"
            );
            jf.get::<JfPage>(&path, &id, "列继续观看").await?.items
        }
        "next_up" => {
            let path = format!(
                "/Shows/NextUp?userId={u}&limit={SHELF}&enableResumable=false{parent}&{LIST_FIELDS}"
            );
            jf.get::<JfPage>(&path, &id, "列下一集").await?.items
        }
        "latest" => {
            // 新加的单集按剧归成一项（groupItems），首页不会被一整季刷屏。
            let path = format!(
                "/Items/Latest?userId={u}&limit={SHELF}&includeItemTypes=Movie,Episode,Video&groupItems=true{parent}&{LIST_FIELDS}"
            );
            jf.get::<Vec<JfEntry>>(&path, &id, "列新加入").await?
        }
        other => return Err(Failure::bad_request(format!("不认识的一排：{other}"))),
    };
    Ok(EntryPage {
        entries: items.iter().map(map::entry).collect(),
        cursor: None,
        total: None,
    })
}

pub async fn detail(jf: &Jellyfin, who: &Account, id: &str) -> Reply<EntryDetail> {
    let u = user(who);
    let ident = who.identity();
    let path = format!("/Items/{}?userId={u}&{DETAIL_FIELDS}", encode(id));
    let item: JfEntry = jf.get(&path, &ident, "读详情").await?;
    let mut out = map::detail(&item);
    match item.kind.as_str() {
        "Series" => {
            let path = format!("/Shows/{}/Seasons?userId={u}&{LIST_FIELDS}", encode(id));
            out.children = list(jf, &path, who, "列季").await?;
            out.next_up = series_next_up(jf, who, id).await?;
        }
        "Season" => {
            let series = item.series_id.as_deref().unwrap_or_default();
            let path = format!(
                "/Shows/{}/Episodes?userId={u}&seasonId={}&{LIST_FIELDS}",
                encode(series),
                encode(id)
            );
            out.children = list(jf, &path, who, "列集").await?;
        }
        _ => {}
    }
    Ok(out)
}

async fn list(
    jf: &Jellyfin,
    path: &str,
    who: &Account,
    what: &str,
) -> Reply<Vec<wire::CatalogEntry>> {
    let page: JfPage = jf.get(path, &who.identity(), what).await?;
    Ok(page.items.iter().map(map::entry).collect())
}

/// 剧的播放键：看了一半的那集，否则下一集没看的（Jellyfin 的 NextUp 两样都给），都看完了就第一集。
async fn series_next_up(
    jf: &Jellyfin,
    who: &Account,
    series: &str,
) -> Reply<Option<wire::CatalogEntry>> {
    let u = user(who);
    let path = format!(
        "/Shows/NextUp?userId={u}&seriesId={}&limit=1&enableResumable=true&enableRewatching=false&{LIST_FIELDS}",
        encode(series)
    );
    if let Some(next) = list(jf, &path, who, "找下一集").await?.into_iter().next() {
        return Ok(Some(next));
    }
    let path = format!(
        "/Shows/{}/Episodes?userId={u}&limit=1&{LIST_FIELDS}",
        encode(series)
    );
    Ok(list(jf, &path, who, "找第一集").await?.into_iter().next())
}

pub async fn genres(jf: &Jellyfin, who: &Account, library: Option<&str>) -> Reply<Genres> {
    let mut path = format!(
        "/Genres?userId={}&includeItemTypes=Movie,Series,Video&sortBy=SortName&sortOrder=Ascending",
        user(who)
    );
    if let Some(l) = library.filter(|l| !l.is_empty()) {
        path.push_str(&format!("&parentId={}", encode(l)));
    }
    let page: JfPage = jf.get(&path, &who.identity(), "列分类").await?;
    Ok(Genres {
        genres: page.items.into_iter().map(|g| g.name).collect(),
    })
}

/// 逐条做，部分成功逐条回失败；整体只在请求本身不对时失败。
pub async fn act(
    jf: &Jellyfin,
    who: &Account,
    action: &CatalogAction,
) -> Reply<CatalogActionResult> {
    let (ids, on, base, what) = match action {
        CatalogAction::SetPlayed { ids, played } => (ids, *played, "/UserPlayedItems", "标已看"),
        CatalogAction::Favorite { ids, favorite } => (ids, *favorite, "/UserFavoriteItems", "收藏"),
    };
    if ids.is_empty() {
        return Err(Failure::bad_request("ids 是空的"));
    }
    let method = if on { Method::POST } else { Method::DELETE };
    let mut result = CatalogActionResult {
        changed: 0,
        failed: Vec::new(),
    };
    for id in ids {
        let path = format!("{base}/{}?userId={}", encode(id), user(who));
        match jf
            .call(method.clone(), &path, None, &who.identity(), what)
            .await
        {
            Ok(()) => result.changed += 1,
            // 凭据失效整体失败，交给外层重拿令牌后重做（POST/DELETE 在 Jellyfin 里都是幂等的）。
            Err(f) if f.code == crate::failure::Code::AuthenticationRequired => return Err(f),
            Err(failure) => result.failed.push(ActionFailure {
                id: id.clone(),
                failure,
            }),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(f: impl FnOnce(&mut EntriesQuery)) -> Reply<String> {
        let mut query = EntriesQuery::default();
        f(&mut query);
        entries_query(&query)
    }

    #[test]
    fn the_wall_lists_the_top_level_of_a_library_by_title() {
        let got = q(|q| q.library = Some("lib".into())).unwrap();
        assert!(got.starts_with(
            "Recursive=true&SortBy=SortName&SortOrder=Ascending&StartIndex=0&Limit=60"
        ));
        assert!(got.contains("&IncludeItemTypes=Movie,Series,Video"));
        assert!(got.contains("&ParentId=lib"));
    }

    #[test]
    fn a_parent_lists_its_direct_children_in_episode_order() {
        let got = q(|q| {
            q.library = Some("lib".into());
            q.parent = Some("series1".into());
            q.cursor = Some("60".into());
        })
        .unwrap();
        assert!(got.starts_with("Recursive=false&SortBy=ParentIndexNumber,IndexNumber,SortName"));
        assert!(got.contains("StartIndex=60"));
        assert!(got.contains("&ParentId=series1") && !got.contains("ParentId=lib"));
        assert!(!got.contains("IncludeItemTypes"));
    }

    #[test]
    fn filters_and_sorts_translate_and_unknown_values_are_rejected() {
        let got = q(|q| {
            q.genre = Some("科幻".into());
            q.q = Some("沙丘 2".into());
            q.sort = Some("added".into());
            q.unplayed = Some(true);
            q.entry_type = Some("movie".into());
        })
        .unwrap();
        assert!(got.contains("SortBy=DateCreated,SortName&SortOrder=Descending"));
        assert!(got.contains("&IncludeItemTypes=Movie&"));
        assert!(got.contains("&Genres=%E7%A7%91%E5%B9%BB"));
        assert!(got.contains("&SearchTerm=%E6%B2%99%E4%B8%98%202"));
        assert!(got.ends_with("&IsPlayed=false"));

        for bad in [
            q(|q| q.sort = Some("runtime".into())),
            q(|q| q.entry_type = Some("trailer".into())),
            q(|q| q.cursor = Some("abc".into())),
        ] {
            assert_eq!(bad.unwrap_err().code, crate::failure::Code::Protocol);
        }
    }

    #[test]
    fn actions_with_unknown_kinds_do_not_parse() {
        let ok: CatalogAction = serde_json::from_value(
            serde_json::json!({"kind": "set_played", "ids": ["a"], "played": true}),
        )
        .unwrap();
        assert_eq!(
            ok,
            CatalogAction::SetPlayed {
                ids: vec!["a".into()],
                played: true
            }
        );
        assert!(
            serde_json::from_value::<CatalogAction>(
                serde_json::json!({"kind": "delete", "ids": ["a"]})
            )
            .is_err()
        );
    }
}
