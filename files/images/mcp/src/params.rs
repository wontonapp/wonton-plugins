//! 工具参数和 schema。

use rmcp::schemars;
use serde::Deserialize;

pub const SHARE_HINT: &str =
    "共享文件夹，可选 documents、photos、music、videos、downloads、transfers。";

#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MatchMode {
    #[default]
    Substring,
    Glob,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum KindParam {
    Image,
    Audio,
    Video,
    Document,
    Text,
    Other,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchFiles {
    #[schemars(description = "要匹配的文件名。match 是 substring 时是子串，是 glob 时是通配符。")]
    pub query: String,
    #[schemars(description = "只在某个共享文件夹里找；不写就找全部六个。")]
    pub share: Option<String>,
    #[schemars(description = "共享文件夹里的子目录，和 share 一起用。")]
    pub subdir: Option<String>,
    #[schemars(description = "名字匹配方式，默认 substring。")]
    #[serde(default)]
    pub r#match: MatchMode,
    #[schemars(description = "区分大小写，默认否。")]
    #[serde(default)]
    pub case_sensitive: bool,
    #[schemars(
        description = "只返回修改时间不早于这个时间的文件，写 YYYY-MM-DD 或 RFC 3339，如 2026-08-01T00:00:00Z；只有日期时按 TZ 解释。"
    )]
    pub modified_after: Option<String>,
    #[schemars(description = "只要这一类文件：image、audio、video、document、text、other。")]
    pub kind: Option<KindParam>,
    #[schemars(description = "跳过前几个匹配，默认 0。")]
    pub offset: Option<u64>,
    #[schemars(description = "这一页最多几个，默认 50，最多 100。")]
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchContent {
    #[schemars(description = "要查找的文字，区分大小写。")]
    pub query: String,
    #[schemars(description = SHARE_HINT)]
    pub share: Option<String>,
    #[schemars(description = "共享文件夹里的子目录，和 share 一起用。")]
    pub subdir: Option<String>,
    #[schemars(description = "只查修改时间不早于这个时间的文件，写 YYYY-MM-DD 或 RFC 3339。")]
    pub modified_after: Option<String>,
    #[schemars(description = "只查这一类文件，取值同 search_files。")]
    pub kind: Option<KindParam>,
    #[schemars(description = "超过这个字节数的文件跳过，默认 1 MiB，最多 8 MiB。")]
    pub max_size: Option<u64>,
    #[schemars(description = "跳过前几个匹配，默认 0。")]
    pub offset: Option<u64>,
    #[schemars(description = "这一页最多几个，默认 50，最多 100。")]
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListDirectory {
    #[schemars(description = SHARE_HINT)]
    pub share: String,
    #[schemars(description = "共享文件夹里的子目录，不写就是共享文件夹的根。")]
    pub path: Option<String>,
    #[schemars(description = "跳过前几项，默认 0。")]
    pub offset: Option<u64>,
    #[schemars(description = "这一页最多几项，默认 50，最多 100。")]
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadTextFile {
    #[schemars(description = SHARE_HINT)]
    pub share: String,
    #[schemars(description = "共享文件夹里的相对路径。")]
    pub path: String,
    #[schemars(description = "从第几个字节开始，默认 0；上一步的结尾会给出下一个值。")]
    pub offset: Option<u64>,
    #[schemars(description = "最多读几个字节，默认 64 KiB，最多 64 KiB。")]
    pub limit: Option<u64>,
}
