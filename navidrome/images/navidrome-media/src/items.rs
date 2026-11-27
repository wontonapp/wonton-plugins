//! `GET /items`：给 Agent 找歌。`q` 走 `search3`；`album`、`artist` 是名字（条目里只带名字），先按名字搜、再按
//! 名字精确筛。游标是偏移量，同一组参数下稳定；顺序沿用 Navidrome 的搜索结果。

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::Value;

use crate::failure::{MediaFailure, Reply};
use crate::upstream::Upstream;
use crate::wire::{ItemsPage, ItemsQuery, MediaItemRef};

const PAGE: usize = 50;
/// 按专辑、歌手名筛时一次最多看这么多首搜索结果。
const SCAN: usize = 500;

/// 路径段里留原样的字符：RFC 3986 的 unreserved。
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

pub(crate) fn segment(id: &str) -> String {
    utf8_percent_encode(id, SEGMENT).to_string()
}

pub(crate) async fn list(
    upstream: &Upstream,
    person: &str,
    query: &ItemsQuery,
) -> Reply<ItemsPage> {
    if query.kind.as_deref().is_some_and(|kind| kind != "audio") {
        return Ok(ItemsPage {
            items: vec![],
            cursor: None,
        });
    }
    let offset = match query.cursor.as_deref() {
        None | Some("") => 0,
        Some(cursor) => cursor
            .parse::<usize>()
            .map_err(|_| MediaFailure::protocol(format!("游标 {cursor} 不是这个适配器发的")))?,
    };
    let q = query.q.as_deref().unwrap_or("");
    let by_name = query.album.as_deref().or(query.artist.as_deref());
    let Some(name) = by_name else {
        let songs = search(upstream, person, q, PAGE, offset).await?;
        let full = songs.len() == PAGE;
        return Ok(page(&songs, full.then(|| offset + PAGE)));
    };
    let matched: Vec<Value> = search(upstream, person, name, SCAN, 0)
        .await?
        .into_iter()
        .filter(|song| {
            same(song, "album", query.album.as_deref())
                && same(song, "artist", query.artist.as_deref())
                && contains(song, "title", q)
        })
        .collect();
    let next = (matched.len() > offset + PAGE).then_some(offset + PAGE);
    let songs: Vec<Value> = matched.into_iter().skip(offset).take(PAGE).collect();
    Ok(page(&songs, next))
}

async fn search(
    upstream: &Upstream,
    person: &str,
    text: &str,
    count: usize,
    offset: usize,
) -> Reply<Vec<Value>> {
    let params = vec![
        ("query", text.to_owned()),
        ("artistCount", "0".to_owned()),
        ("albumCount", "0".to_owned()),
        ("songCount", count.to_string()),
        ("songOffset", offset.to_string()),
    ];
    let reply = upstream.call(person, "search3", &params).await?;
    Ok(reply
        .pointer("/searchResult3/song")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn page(songs: &[Value], next: Option<usize>) -> ItemsPage {
    ItemsPage {
        items: songs.iter().filter_map(song_item).collect(),
        cursor: next.map(|offset| offset.to_string()),
    }
}

fn same(song: &Value, field: &str, wanted: Option<&str>) -> bool {
    wanted.is_none_or(|wanted| {
        song.get(field)
            .and_then(Value::as_str)
            .is_some_and(|value| value.to_lowercase() == wanted.to_lowercase())
    })
}

fn contains(song: &Value, field: &str, needle: &str) -> bool {
    needle.is_empty()
        || song
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(|value| value.to_lowercase().contains(&needle.to_lowercase()))
}

/// 一首歌的 Subsonic `Child` 翻成条目。封面不给上游地址（手机连不到），走 `/items/{id}/artwork`。
pub(crate) fn song_item(song: &Value) -> Option<MediaItemRef> {
    let text = |field: &str| song.get(field).and_then(Value::as_str).map(str::to_owned);
    let number = |field: &str| song.get(field).and_then(Value::as_u64);
    Some(MediaItemRef {
        id: text("id")?,
        kind: "audio".to_owned(),
        title: text("title").unwrap_or_default(),
        artist: text("artist"),
        album: text("album"),
        artwork_url: None,
        duration_ms: number("duration").map(|secs| secs * 1000),
        series: None,
        index: number("track").and_then(|track| u32::try_from(track).ok()),
        played: number("playCount").is_some_and(|count| count > 0),
        starred: song.get("starred").is_some(),
    })
}
