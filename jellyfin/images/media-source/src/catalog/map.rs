//! Jellyfin 的条目（`BaseItemDto`）翻成浏览回包。纯函数，不发请求。

use serde::Deserialize;
use serde_json::Value;

use crate::contract::MediaTrack;
use crate::jellyfin::ticks_to_ms;

use super::wire::{CatalogEntry, CatalogLibrary, Credit, EntryDetail, MediaInfo};

/// 列表和详情都要的字段；`ImageTypeLimit=1` 让背景图只回一张的标记。
pub const LIST_FIELDS: &str = "Fields=ChildCount,DateCreated&EnableUserData=true&EnableImageTypes=Primary,Backdrop,Thumb,Logo&ImageTypeLimit=1";
pub const DETAIL_FIELDS: &str = "Fields=ChildCount,DateCreated,Overview,Taglines,Genres,Studios,People,PremiereDate,MediaSources,MediaStreams&EnableUserData=true&EnableImageTypes=Primary,Backdrop,Thumb,Logo&ImageTypeLimit=1";

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct JfEntry {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "Type", default)]
    pub kind: String,
    pub collection_type: Option<String>,
    pub production_year: Option<u16>,
    pub series_id: Option<String>,
    pub series_name: Option<String>,
    pub parent_index_number: Option<u32>,
    pub index_number: Option<u32>,
    pub run_time_ticks: Option<u64>,
    pub child_count: Option<u32>,
    pub community_rating: Option<f32>,
    pub official_rating: Option<String>,
    pub date_created: Option<String>,
    pub premiere_date: Option<String>,
    #[serde(default)]
    pub image_tags: Option<Value>,
    #[serde(default)]
    pub backdrop_image_tags: Vec<String>,
    #[serde(default)]
    pub user_data: Option<JfUserData>,
    pub overview: Option<String>,
    #[serde(default)]
    pub taglines: Vec<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub studios: Vec<JfNamed>,
    #[serde(default)]
    pub people: Vec<JfPerson>,
    #[serde(default)]
    pub media_sources: Vec<JfSource>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct JfUserData {
    #[serde(default)]
    pub played: bool,
    #[serde(default)]
    pub is_favorite: bool,
    #[serde(default)]
    pub playback_position_ticks: u64,
    pub unplayed_item_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct JfNamed {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct JfPerson {
    pub name: String,
    pub id: Option<String>,
    pub role: Option<String>,
    #[serde(rename = "Type")]
    pub kind: Option<String>,
    pub primary_image_tag: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct JfSource {
    pub container: Option<String>,
    pub size: Option<u64>,
    #[serde(default)]
    pub media_streams: Vec<JfStream>,
    pub default_audio_stream_index: Option<i32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct JfStream {
    #[serde(rename = "Type")]
    pub kind: String,
    pub index: i32,
    pub language: Option<String>,
    pub display_title: Option<String>,
    pub codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_range: Option<String>,
    pub channels: Option<u8>,
    #[serde(default)]
    pub is_default: bool,
}

/// Jellyfin 的类型名翻成契约的层级；认不得的照原样小写，客户端落到 `Other`。
pub fn entry_type(kind: &str) -> String {
    match kind {
        "Movie" => "movie",
        "Series" => "series",
        "Season" => "season",
        "Episode" => "episode",
        "Video" | "MusicVideo" => "video",
        "Folder" | "CollectionFolder" | "UserView" => "folder",
        other => return other.to_ascii_lowercase(),
    }
    .to_owned()
}

/// 契约的层级翻回 Jellyfin 的类型名；不认识的回 `None`，调用方按协议错回。
pub fn jellyfin_type(entry_type: &str) -> Option<&'static str> {
    Some(match entry_type {
        "movie" => "Movie",
        "series" => "Series",
        "season" => "Season",
        "episode" => "Episode",
        "video" => "Video",
        "folder" => "Folder",
        _ => return None,
    })
}

/// 片库：只要放视频的那几种。音乐、相册、书、播放列表不在影视页里出现。
pub fn library(view: &JfEntry) -> Option<CatalogLibrary> {
    let kind = match view.collection_type.as_deref() {
        Some("movies") => "movies",
        Some("tvshows") => "shows",
        Some("homevideos") | Some("musicvideos") | Some("mixed") | None => "mixed",
        Some(_) => return None,
    };
    Some(CatalogLibrary {
        id: view.id.clone(),
        title: view.name.clone(),
        kind,
        count: view.child_count,
    })
}

pub fn entry(item: &JfEntry) -> CatalogEntry {
    let user = item.user_data.as_ref();
    let played = user.is_some_and(|u| u.played);
    let (season, episode) = match item.kind.as_str() {
        "Episode" => (item.parent_index_number, item.index_number),
        "Season" => (item.index_number, None),
        _ => (None, None),
    };
    let position = user
        .map(|u| ticks_to_ms(u.playback_position_ticks))
        .filter(|ms| *ms > 0 && !played);
    CatalogEntry {
        id: item.id.clone(),
        entry_type: entry_type(&item.kind),
        title: item.name.clone(),
        year: item.production_year,
        series_id: item.series_id.clone(),
        series_title: item.series_name.clone(),
        season,
        episode,
        duration_ms: item.run_time_ticks.map(ticks_to_ms),
        position_ms: position,
        played,
        favorite: user.is_some_and(|u| u.is_favorite),
        unplayed_count: user.and_then(|u| u.unplayed_item_count),
        child_count: item.child_count,
        community_rating: item.community_rating,
        official_rating: item.official_rating.clone(),
        added_at: item.date_created.as_deref().and_then(iso_ms),
        artwork: artwork(item),
    }
}

fn artwork(item: &JfEntry) -> Vec<&'static str> {
    let tags = item.image_tags.as_ref();
    let has = |name: &str| tags.and_then(|t| t.get(name)).is_some();
    let mut out = Vec::new();
    if has("Primary") {
        out.push("primary");
    }
    if !item.backdrop_image_tags.is_empty() {
        out.push("backdrop");
    }
    if has("Thumb") {
        out.push("thumb");
    }
    if has("Logo") {
        out.push("logo");
    }
    out
}

/// 详情；`children` 和 `next_up` 由调用方按类型另取后填。
pub fn detail(item: &JfEntry) -> EntryDetail {
    let source = item.media_sources.first();
    let (audio, subtitles) = source.map(tracks).unwrap_or_default();
    EntryDetail {
        entry: entry(item),
        overview: item.overview.clone().filter(|s| !s.is_empty()),
        tagline: item.taglines.first().cloned(),
        genres: item.genres.clone(),
        studios: item.studios.iter().map(|s| s.name.clone()).collect(),
        premiered_at: item.premiere_date.as_deref().and_then(iso_ms),
        people: item.people.iter().map(credit).collect(),
        audio,
        subtitles,
        media: source.map(media_info),
        ..EntryDetail::default()
    }
}

fn credit(person: &JfPerson) -> Credit {
    let role = match person.kind.as_deref() {
        Some("Actor") | Some("GuestStar") => "actor".to_owned(),
        Some(other) => other.to_ascii_lowercase(),
        None => "actor".to_owned(),
    };
    Credit {
        id: person.id.clone(),
        name: person.name.clone(),
        character: person
            .role
            .clone()
            .filter(|r| !r.is_empty() && role == "actor"),
        role,
        artwork: person.primary_image_tag.is_some(),
    }
}

/// 播放前的选择：音轨默认那条标选中，字幕默认都不选（起播后以播放计划为准）。
fn tracks(source: &JfSource) -> (Vec<MediaTrack>, Vec<MediaTrack>) {
    let pick = |kind: &str, label: &'static str, chosen: Option<i32>| -> Vec<MediaTrack> {
        source
            .media_streams
            .iter()
            .filter(|s| s.kind == kind)
            .filter_map(|s| {
                Some(MediaTrack {
                    index: u16::try_from(s.index).ok()?,
                    language: s.language.clone().unwrap_or_else(|| "und".into()),
                    label: s.display_title.clone(),
                    kind: label,
                    selected: chosen == Some(s.index),
                })
            })
            .collect()
    };
    (
        pick("Audio", "audio", source.default_audio_stream_index),
        pick("Subtitle", "subtitle", None),
    )
}

fn media_info(source: &JfSource) -> MediaInfo {
    let video = source.media_streams.iter().find(|s| s.kind == "Video");
    let audio = source
        .media_streams
        .iter()
        .filter(|s| s.kind == "Audio")
        .find(|s| Some(s.index) == source.default_audio_stream_index)
        .or_else(|| source.media_streams.iter().find(|s| s.kind == "Audio"));
    MediaInfo {
        width: video.and_then(|v| v.width),
        height: video.and_then(|v| v.height),
        hdr: video
            .and_then(|v| v.video_range.as_deref())
            .is_some_and(|r| r != "SDR" && r != "Unknown"),
        video_codec: video.and_then(|v| v.codec.clone()),
        audio_codec: audio.and_then(|a| a.codec.clone()),
        audio_channels: audio.and_then(|a| a.channels),
        container: source.container.clone(),
        size: source.size,
    }
}

/// Jellyfin 的时间是 UTC 的 ISO 8601（`2026-11-27T10:12:13.1234567Z`）；解不了的当没有。
pub fn iso_ms(raw: &str) -> Option<u64> {
    let b = raw.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| raw.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, s) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    // 公历日期到 1970-01-01 起的天数（Howard Hinnant 的 days_from_civil）。
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + h * 3600 + mi * 60 + s;
    u64::try_from(secs).ok().map(|s| s * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: serde_json::Value) -> JfEntry {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn an_episode_keeps_its_season_number_and_resume_point() {
        let e = entry(&parse(serde_json::json!({
            "Id": "e1", "Name": "第二集", "Type": "Episode", "SeriesId": "s1", "SeriesName": "演示剧",
            "ParentIndexNumber": 2, "IndexNumber": 1, "RunTimeTicks": 300_000_000u64,
            "DateCreated": "2026-11-27T10:00:00.1234567Z",
            "ImageTags": {"Primary": "x"}, "BackdropImageTags": [],
            "UserData": {"Played": false, "IsFavorite": true, "PlaybackPositionTicks": 120_000_000u64}
        })));
        assert_eq!(e.entry_type, "episode");
        assert_eq!((e.season, e.episode), (Some(2), Some(1)));
        assert_eq!(e.position_ms, Some(12_000));
        assert_eq!(e.duration_ms, Some(30_000));
        assert!(e.favorite && !e.played);
        assert_eq!(e.artwork, vec!["primary"]);
        assert_eq!(e.added_at, Some(1_795_773_600_000));
    }

    #[test]
    fn a_finished_item_reports_no_resume_point() {
        let e = entry(&parse(serde_json::json!({
            "Id": "m1", "Name": "片", "Type": "Movie",
            "UserData": {"Played": true, "PlaybackPositionTicks": 50}
        })));
        assert!(e.played);
        assert_eq!(e.position_ms, None);
    }

    #[test]
    fn a_season_reports_its_number_and_unplayed_count() {
        let e = entry(&parse(serde_json::json!({
            "Id": "se", "Name": "第 1 季", "Type": "Season", "IndexNumber": 1, "ChildCount": 2,
            "BackdropImageTags": ["b"], "UserData": {"UnplayedItemCount": 2}
        })));
        assert_eq!((e.season, e.episode), (Some(1), None));
        assert_eq!(e.unplayed_count, Some(2));
        assert_eq!(e.child_count, Some(2));
        assert_eq!(e.artwork, vec!["backdrop"]);
    }

    #[test]
    fn types_map_both_ways_and_unknown_ones_pass_through_lowercased() {
        for (jf, ours) in [
            ("Movie", "movie"),
            ("Series", "series"),
            ("Season", "season"),
            ("Episode", "episode"),
            ("Video", "video"),
        ] {
            assert_eq!(entry_type(jf), ours);
            assert_eq!(jellyfin_type(ours), Some(jf));
        }
        assert_eq!(entry_type("CollectionFolder"), "folder");
        assert_eq!(entry_type("BoxSet"), "boxset");
        assert_eq!(jellyfin_type("trailer"), None);
    }

    #[test]
    fn only_video_libraries_are_listed() {
        let view = |t: Option<&str>| {
            let mut v = serde_json::json!({"Id": "v", "Name": "库"});
            if let Some(t) = t {
                v["CollectionType"] = t.into();
            }
            library(&parse(v)).map(|l| l.kind)
        };
        assert_eq!(view(Some("movies")), Some("movies"));
        assert_eq!(view(Some("tvshows")), Some("shows"));
        assert_eq!(view(Some("homevideos")), Some("mixed"));
        assert_eq!(view(None), Some("mixed"));
        assert_eq!(view(Some("music")), None);
        assert_eq!(view(Some("photos")), None);
    }

    #[test]
    fn detail_reads_tracks_people_and_media_badges() {
        let d = detail(&parse(serde_json::json!({
            "Id": "m", "Name": "片", "Type": "Movie", "Overview": "", "Taglines": ["一句话"],
            "Genres": ["剧情"], "Studios": [{"Name": "某厂"}], "PremiereDate": "2020-01-01T00:00:00.0000000Z",
            "People": [
                {"Name": "演员甲", "Id": "p1", "Role": "姐姐", "Type": "Actor", "PrimaryImageTag": "t"},
                {"Name": "导演乙", "Id": "p2", "Role": "Director", "Type": "Director"}
            ],
            "MediaSources": [{
                "Container": "mkv", "Size": 1000, "DefaultAudioStreamIndex": 2,
                "MediaStreams": [
                    {"Type": "Video", "Index": 0, "Codec": "hevc", "Width": 3840, "Height": 2160, "VideoRange": "HDR"},
                    {"Type": "Audio", "Index": 1, "Codec": "aac", "Channels": 2, "Language": "eng"},
                    {"Type": "Audio", "Index": 2, "Codec": "ac3", "Channels": 6, "Language": "zho", "DisplayTitle": "国语 5.1"},
                    {"Type": "Subtitle", "Index": 3, "Language": "zho", "IsDefault": true}
                ]
            }]
        })));
        assert_eq!(d.overview, None);
        assert_eq!(d.tagline.as_deref(), Some("一句话"));
        assert_eq!(d.premiered_at, Some(1_577_836_800_000));
        assert_eq!(d.people[0].character.as_deref(), Some("姐姐"));
        assert!(d.people[0].artwork);
        assert_eq!(d.people[1].role, "director");
        assert_eq!(d.people[1].character, None);
        assert_eq!(d.audio.len(), 2);
        assert!(d.audio[1].selected && !d.audio[0].selected);
        assert!(d.subtitles.iter().all(|s| !s.selected));
        let media = d.media.unwrap();
        assert!(media.hdr);
        assert_eq!(
            (
                media.width,
                media.audio_codec.as_deref(),
                media.audio_channels
            ),
            (Some(3840), Some("ac3"), Some(6))
        );
    }

    #[test]
    fn dates_parse_as_utc_milliseconds() {
        assert_eq!(iso_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            iso_ms("2000-03-01T00:00:00.0000000Z"),
            Some(951_868_800_000)
        );
        assert_eq!(iso_ms("not a date"), None);
        assert_eq!(iso_ms("2020-13-01T00:00:00Z"), None);
    }
}
