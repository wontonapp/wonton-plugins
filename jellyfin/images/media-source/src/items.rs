//! `GET /items`（给 Agent 找片）与 `GET /items/{id}/segments`（章节、片头片尾）。
//!
//! 只列能直接放的条目：电影和单集。剧本身不能放，按 `series` 查它的各集。`album`/`artist` 是音乐的
//! 参数，`from`/`to` 是相册的拍摄时间，影视这边不认，带了也只按其余参数列（规范：提供方不支持的参数忽略）。

use serde::Deserialize;
use serde_json::Value;

use crate::accounts::Account;
use crate::contract::{ItemsPage, MediaItemRef, Segment};
use crate::failure::{Failure, Reply};
use crate::jellyfin::{Jellyfin, encode, ticks_to_ms};

pub const PAGE: usize = 50;
const PLAYABLE: &str = "Movie,Episode";
const FIELDS: &str = "Fields=Path&EnableImageTypes=Primary&EnableUserData=true";

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    pub kind: Option<String>,
    pub q: Option<String>,
    pub series: Option<String>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct JfItem {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "Type", default)]
    pub kind: String,
    #[serde(default)]
    pub series_name: Option<String>,
    #[serde(default)]
    pub index_number: Option<u32>,
    #[serde(default)]
    pub run_time_ticks: Option<u64>,
    #[serde(default)]
    pub image_tags: Option<Value>,
    #[serde(default)]
    pub user_data: Option<JfUserData>,
    #[serde(default)]
    pub chapters: Vec<JfChapter>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct JfUserData {
    #[serde(default)]
    pub played: bool,
    #[serde(default)]
    pub is_favorite: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct JfChapter {
    pub start_position_ticks: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JfPage {
    items: Vec<JfItem>,
    #[serde(default)]
    total_record_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JfSegments {
    items: Vec<JfSegment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JfSegment {
    #[serde(rename = "Type")]
    kind: String,
    start_ticks: u64,
    end_ticks: u64,
}

/// 一条 Jellyfin 条目翻成契约的引用。海报地址是本提供方的相对路径，由核心改写成手机能到的地址。
pub fn item_ref(item: &JfItem) -> MediaItemRef {
    let has_poster = item
        .image_tags
        .as_ref()
        .and_then(|tags| tags.get("Primary"))
        .is_some();
    let user = item.user_data.as_ref();
    MediaItemRef {
        id: item.id.clone(),
        kind: "video".into(),
        title: item.name.clone(),
        artist: None,
        album: None,
        artwork_url: has_poster.then(|| format!("/items/{}/artwork", item.id)),
        duration_ms: item.run_time_ticks.map(ticks_to_ms),
        series: item.series_name.clone(),
        index: item.index_number,
        played: user.is_some_and(|u| u.played),
        starred: user.is_some_and(|u| u.is_favorite),
    }
}

fn offset(cursor: Option<&str>) -> Reply<usize> {
    match cursor {
        None | Some("") => Ok(0),
        Some(c) => c
            .parse()
            .map_err(|_| Failure::bad_request(format!("cursor 不是本提供方发的：{c:?}"))),
    }
}

pub async fn list(jf: &Jellyfin, who: &Account, query: &ListQuery) -> Reply<ItemsPage> {
    if query.kind.as_deref().is_some_and(|k| k != "video") {
        return Ok(ItemsPage {
            items: Vec::new(),
            cursor: None,
        });
    }
    let start = offset(query.cursor.as_deref())?;
    let parent = match query.series.as_deref().filter(|s| !s.is_empty()) {
        None => None,
        Some(series) => match series_id(jf, who, series).await? {
            Some(id) => Some(id),
            None => {
                return Ok(ItemsPage {
                    items: Vec::new(),
                    cursor: None,
                });
            }
        },
    };
    let types = if parent.is_some() {
        "Episode"
    } else {
        PLAYABLE
    };
    let mut path = format!(
        "/Items?userId={}&Recursive=true&IncludeItemTypes={types}&SortBy=SortName&SortOrder=Ascending&StartIndex={start}&Limit={PAGE}&{FIELDS}",
        encode(&who.user_id)
    );
    if let Some(id) = parent {
        path.push_str(&format!("&ParentId={}", encode(&id)));
    }
    if let Some(q) = query.q.as_deref().filter(|q| !q.is_empty()) {
        path.push_str(&format!("&SearchTerm={}", encode(q)));
    }
    let page: JfPage = jf.get(&path, &who.identity(), "列片").await?;
    let next = start + page.items.len();
    Ok(ItemsPage {
        items: page.items.iter().map(item_ref).collect(),
        cursor: (next < page.total_record_count && !page.items.is_empty())
            .then(|| next.to_string()),
    })
}

/// 按名字找剧（不分大小写、全名相等），找不到回 `None`：契约里 `series` 是名字不是 id。
async fn series_id(jf: &Jellyfin, who: &Account, name: &str) -> Reply<Option<String>> {
    let path = format!(
        "/Items?userId={}&Recursive=true&IncludeItemTypes=Series&SearchTerm={}&Limit=20",
        encode(&who.user_id),
        encode(name)
    );
    let page: JfPage = jf.get(&path, &who.identity(), "找剧").await?;
    Ok(page
        .items
        .into_iter()
        .find(|s| s.name.to_lowercase() == name.to_lowercase())
        .map(|s| s.id))
}

pub async fn item(jf: &Jellyfin, who: &Account, id: &str) -> Reply<JfItem> {
    let path = format!(
        "/Items/{}?userId={}&Fields=Chapters&EnableUserData=true",
        encode(id),
        encode(&who.user_id)
    );
    // 12.4 的单条接口是 `/Items/{id}`（带 userId）；`/Users/{id}/Items/{id}` 已从 OpenAPI 移除。
    jf.get(&path, &who.identity(), "读这一条").await
}

/// 片头片尾来自 Jellyfin 的媒体段（要装片头识别插件才有），章节来自文件自带的章节标记；两样都没有回 404。
pub async fn segments(jf: &Jellyfin, who: &Account, id: &str) -> Reply<Vec<Segment>> {
    let entry = item(jf, who, id).await?;
    let path = format!("/MediaSegments/{}", encode(id));
    let marked: JfSegments = jf.get(&path, &who.identity(), "读片头片尾").await?;
    let mut out: Vec<Segment> = marked
        .items
        .iter()
        .map(|s| Segment {
            kind: segment_kind(&s.kind),
            start_ms: ticks_to_ms(s.start_ticks),
            end_ms: ticks_to_ms(s.end_ticks),
        })
        .collect();
    out.extend(chapters(&entry));
    if out.is_empty() {
        return Err(Failure::new(
            crate::failure::Code::MediaGone,
            "这一条没有章节和片头片尾",
        ));
    }
    out.sort_by_key(|s| s.start_ms);
    Ok(out)
}

fn segment_kind(jellyfin: &str) -> String {
    match jellyfin {
        "Intro" => "intro".into(),
        "Outro" => "outro".into(),
        other => other.to_ascii_lowercase(),
    }
}

/// 章节只有起点；终点是下一章的起点，最后一章到片尾。
pub fn chapters(item: &JfItem) -> Vec<Segment> {
    let end = item.run_time_ticks.map(ticks_to_ms).unwrap_or(0);
    let starts: Vec<u64> = item
        .chapters
        .iter()
        .map(|c| ticks_to_ms(c.start_position_ticks))
        .collect();
    starts
        .iter()
        .enumerate()
        .map(|(i, &start)| Segment {
            kind: "chapter".into(),
            start_ms: start,
            end_ms: starts.get(i + 1).copied().unwrap_or(end).max(start),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: Value) -> JfItem {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn episode_maps_series_index_and_user_state() {
        let item = parse(json!({
            "Id": "e1", "Name": "第一集", "Type": "Episode", "SeriesName": "示例剧", "IndexNumber": 1,
            "RunTimeTicks": 12_000_000_000u64, "ImageTags": {"Primary": "abc"},
            "UserData": {"Played": true, "IsFavorite": false}
        }));
        let r = item_ref(&item);
        assert_eq!(r.kind, "video");
        assert_eq!(r.series.as_deref(), Some("示例剧"));
        assert_eq!(r.index, Some(1));
        assert_eq!(r.duration_ms, Some(1_200_000));
        assert_eq!(r.artwork_url.as_deref(), Some("/items/e1/artwork"));
        assert!(r.played);
        assert!(!r.starred);
    }

    #[test]
    fn no_poster_no_artwork_url() {
        let r = item_ref(&parse(json!({"Id": "m", "Name": "M", "Type": "Movie"})));
        assert_eq!(r.artwork_url, None);
        assert!(!r.played);
    }

    #[test]
    fn chapters_end_where_the_next_begins() {
        let item = parse(json!({
            "Id": "m", "Type": "Movie", "RunTimeTicks": 300_000_000u64,
            "Chapters": [{"StartPositionTicks": 0}, {"StartPositionTicks": 100_000_000u64}]
        }));
        let got = chapters(&item);
        assert_eq!(got.len(), 2);
        assert_eq!((got[0].start_ms, got[0].end_ms), (0, 10_000));
        assert_eq!((got[1].start_ms, got[1].end_ms), (10_000, 30_000));
    }

    #[test]
    fn foreign_cursor_is_rejected() {
        assert!(offset(Some("abc")).is_err());
        assert_eq!(offset(None).unwrap(), 0);
        assert_eq!(offset(Some("50")).unwrap(), 50);
    }
}
