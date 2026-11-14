//! 工具结果的统一形状：给人看的文字加机读的 `structuredContent`。
//! 测试断言结构化字段，不依赖措辞。

use std::ffi::OsString;

use rmcp::model::{CallToolResult, ContentBlock, Resource};
use serde_json::{Value, json};

use crate::fs::{FsError, display, file_name, link_uri, mime_of};
use crate::report::{Notes, WalkReport, note_value, report_lines, report_value};

pub const DEFAULT_PAGE: u64 = 50;
pub const MAX_PAGE: u64 = 100;

#[derive(Default)]
pub struct ContentCounters {
    pub too_large: Notes,
    pub unreadable: Notes,
    pub binary: Notes,
    pub not_utf8: Notes,
    pub files: u64,
    pub bytes: u64,
    pub stopped: bool,
}

pub enum Scan {
    Match,
    Skip,
    Stop,
}

pub fn page_limit(limit: Option<u64>) -> u64 {
    limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE)
}

pub fn window(total: u64, offset: u64, limit: u64) -> (usize, usize) {
    let start = offset.min(total) as usize;
    let end = (start as u64 + limit).min(total) as usize;
    (start, end)
}

pub fn link(share: &str, rel: &[OsString], stat: &rustix::fs::Stat) -> ContentBlock {
    let name = file_name(rel);
    let mut resource = Resource::new(link_uri(share, rel), &name)
        .with_title(display(share, rel))
        .with_mime_type(mime_of(&name));
    if stat.st_size >= 0 {
        resource = resource.with_size(stat.st_size as u64);
    }
    ContentBlock::resource_link(resource)
}

pub fn error(kind: &str, message: impl Into<String>) -> CallToolResult {
    let message = message.into();
    let mut result = CallToolResult::error(vec![ContentBlock::text(message.clone())]);
    result.structured_content = Some(json!({ "error": kind, "message": message }));
    result
}

pub fn fs_error(err: FsError) -> CallToolResult {
    let kind = match &err {
        FsError::Invalid(_) => "invalid_path",
        FsError::InvalidParam(_) => "invalid_param",
        FsError::TimezoneUnavailable(_) => "timezone_unavailable",
        FsError::NotFound(_) => "not_found",
        FsError::Denied(_) => "denied",
        FsError::TooLarge(_) => "too_large",
        FsError::NotText => "not_text",
        FsError::NotUtf8 => "not_utf8",
        FsError::Io(_) => "io",
    };
    error(kind, err.to_string())
}

pub fn success(content: Vec<ContentBlock>, structured: Value) -> CallToolResult {
    let mut result = CallToolResult::success(content);
    result.structured_content = Some(structured);
    result
}

pub fn page_result<T>(
    items: Vec<T>,
    offset: u64,
    limit: u64,
    report: &WalkReport,
    counters: Option<&ContentCounters>,
    extra_lines: Vec<String>,
    link: impl Fn(&T) -> ContentBlock,
) -> CallToolResult {
    let total = items.len() as u64;
    let (start, end) = window(total, offset, limit);
    let next = (end < total as usize).then_some(end as u64);
    let mut content = Vec::new();
    if total == 0 {
        content.push(ContentBlock::text("没有匹配的文件。"));
    } else {
        content.push(ContentBlock::text(format!(
            "匹配 {total} 个文件，显示第 {}–{} 个。",
            start + 1,
            end
        )));
        for item in &items[start..end] {
            content.push(link(item));
        }
        if let Some(next) = next {
            content.push(ContentBlock::text(format!(
                "后面还有 {} 个，用 offset={next} 继续。",
                total as usize - end
            )));
        }
    }
    content.extend(extra_lines.into_iter().map(ContentBlock::text));
    content.extend(report_lines(report).into_iter().map(ContentBlock::text));
    content.push(ContentBlock::text(
        "结果里的文件要发给你时，对每个文件调 wonton__share_file，把编号原样放进回复。",
    ));

    let mut notes = report_value(report);
    if let Some(counters) = counters
        && let Some(object) = notes.as_object_mut()
    {
        object.insert("too_large".to_owned(), note_value(&counters.too_large));
        object.insert("unreadable".to_owned(), note_value(&counters.unreadable));
        object.insert("binary".to_owned(), note_value(&counters.binary));
        object.insert("not_utf8".to_owned(), note_value(&counters.not_utf8));
    }
    success(
        content,
        json!({
            "total": total,
            "offset": start,
            "returned": end - start,
            "next_offset": next,
            "stopped": report.stopped,
            "notes": notes,
        }),
    )
}

pub fn read_result(
    share: &str,
    rel: &[OsString],
    size: u64,
    offset: u64,
    skip: u64,
    text: &str,
    next: Option<u64>,
) -> CallToolResult {
    let mut content = vec![ContentBlock::text(format!(
        "{}（{size} 字节）",
        display(share, rel)
    ))];
    if skip > 0 {
        content.push(ContentBlock::text(format!(
            "offset {offset} 在字符中间，从 {} 开始。",
            offset + skip
        )));
    }
    if size == 0 {
        content.push(ContentBlock::text("空文件".to_owned()));
    } else {
        content.push(ContentBlock::text(text.to_owned()));
        match next {
            Some(next) => content.push(ContentBlock::text(format!(
                "还有内容，用 offset={next} 继续。"
            ))),
            None => content.push(ContentBlock::text("已到末尾。".to_owned())),
        }
    }
    success(
        content,
        json!({
            "size": size,
            "offset": offset,
            "start": offset + skip,
            "next_offset": next,
            "text": text,
        }),
    )
}
