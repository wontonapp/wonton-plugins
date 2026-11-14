//! MCP 工具：只读地找文件、列目录、读文字。每个结果里的文件都是 `wonton-share:` 引用。

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;
use std::time::Duration;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ServerCapabilities, ServerConfig};
use rmcp::{ErrorData as McpError, ServerHandler, tool, tool_handler, tool_router};
use serde_json::json;
use tokio::sync::Semaphore;

use crate::fs::{
    Budget, FilesFs, Found, FsError, Kind, SHARES, TextKind, classify_text, display, file_name,
    kind_of, link_uri, parse_time,
};
use crate::matcher::Matcher;
use crate::output::{
    ContentCounters, Scan, fs_error, link, page_limit, page_result, read_result, window,
};
pub use crate::params::{
    KindParam, ListDirectory, MatchMode, ReadTextFile, SearchContent, SearchFiles,
};
use crate::report::{WalkReport, merge, push_note, report_lines, report_value};

const MAX_READ: u64 = 64 * 1024;
const DEFAULT_CONTENT_SIZE: u64 = 1024 * 1024;
const MAX_CONTENT_SIZE: u64 = 8 * 1024 * 1024;
const SEARCH_SLOTS: usize = 4;

type DirItem = (OsString, Option<(Vec<OsString>, rustix::fs::Stat)>);

/// 一次搜索的工作量上限，达到后停下并在结果里说明。
#[derive(Debug, Clone)]
pub struct Limits {
    pub entries: u64,
    pub matches: usize,
    pub content_files: u64,
    pub content_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: 200_000,
            matches: 10_000,
            content_files: 2_000,
            content_bytes: 64 * 1024 * 1024,
        }
    }
}

impl From<KindParam> for Kind {
    fn from(value: KindParam) -> Self {
        match value {
            KindParam::Image => Self::Image,
            KindParam::Audio => Self::Audio,
            KindParam::Video => Self::Video,
            KindParam::Document => Self::Document,
            KindParam::Text => Self::Text,
            KindParam::Other => Self::Other,
        }
    }
}

struct Scope {
    shares: Vec<(&'static str, Vec<OsString>)>,
    after: Option<i64>,
    kind: Option<Kind>,
    offset: u64,
    limit: u64,
}

#[derive(Clone)]
pub struct FilesServer {
    fs: Arc<FilesFs>,
    limits: Arc<Limits>,
    search_slots: Arc<Semaphore>,
    delay: Option<Duration>,
    tool_router: ToolRouter<Self>,
}

fn list_item(
    dir: &std::os::fd::OwnedFd,
    share: &str,
    base: &[OsString],
    name: OsString,
    report: &mut WalkReport,
) -> Option<DirItem> {
    let mut rel = base.to_vec();
    rel.push(name.clone());
    let here = display(share, &rel);
    if name.to_str().is_none() {
        report.non_utf8.add(here.clone());
    }
    match rustix::fs::statat(dir, name.as_os_str(), rustix::fs::AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat)
            if rustix::fs::FileType::from_raw_mode(stat.st_mode)
                == rustix::fs::FileType::RegularFile =>
        {
            Some((name, Some((rel, stat))))
        }
        Ok(stat)
            if rustix::fs::FileType::from_raw_mode(stat.st_mode)
                == rustix::fs::FileType::Directory =>
        {
            Some((format!("{}/", name.to_string_lossy()).into(), None))
        }
        Ok(_) => Some((
            format!("{}（软链接或特殊文件）", name.to_string_lossy()).into(),
            None,
        )),
        Err(rustix::io::Errno::ACCESS | rustix::io::Errno::PERM) => {
            report.denied.add(here);
            None
        }
        Err(_) => {
            report.stat_failed.add(here);
            None
        }
    }
}

#[tool_router]
impl FilesServer {
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self::with_limits(root, Limits::default())
    }

    pub fn with_limits(root: impl Into<std::path::PathBuf>, limits: Limits) -> Self {
        Self {
            fs: Arc::new(FilesFs::new(root)),
            limits: Arc::new(limits),
            search_slots: Arc::new(Semaphore::new(SEARCH_SLOTS)),
            delay: None,
            tool_router: Self::tool_router(),
        }
    }

    /// 测试用：让搜索先睡一会儿，模拟慢盘。
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    pub fn fs(&self) -> &FilesFs {
        &self.fs
    }

    pub fn tool_defs(&self) -> Vec<rmcp::model::Tool> {
        self.tool_router.list_all()
    }

    #[tool(
        annotations(read_only_hint = true, title = "按文件名找文件"),
        description = "在共享文件夹里按文件名（子串或 glob）找文件，可以限定共享文件夹、子目录、修改时间和类型。结果分页，每页最多 100 个。跳过的条目和达到的上限会写在结果里。每个文件返回成 wonton-share 引用，带 name、mimeType、size。要发给你时，对结果里的文件调 wonton__share_file。"
    )]
    pub async fn search_files(
        &self,
        Parameters(params): Parameters<SearchFiles>,
    ) -> Result<CallToolResult, McpError> {
        let permit = match self.search_slots.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(err) => return Ok(output_error(err.to_string())),
        };
        self.spawn_tool(move |server| {
            let _permit = permit;
            server.search_files_blocking(params)
        })
        .await
    }

    fn search_files_blocking(&self, params: SearchFiles) -> Result<CallToolResult, McpError> {
        if let Some(delay) = self.delay {
            std::thread::sleep(delay);
        }
        let scope = match self.scope(
            params.share.as_deref(),
            params.subdir.as_deref(),
            params.modified_after.as_deref(),
            params.kind,
            params.offset,
            params.limit,
        ) {
            Ok(scope) => scope,
            Err(err) => return Ok(fs_error(err)),
        };
        let matcher = match Matcher::new(&params.query, params.r#match, params.case_sensitive) {
            Ok(matcher) => matcher,
            Err(msg) => return Ok(output_error(msg)),
        };

        let mut found = Vec::new();
        let mut report = WalkReport::default();
        let mut budget = Budget::new(self.limits.entries);
        for (share, base) in &scope.shares {
            let walk = self.fs.walk(share, base, &mut budget, |item| {
                if !self.matches(item, &scope, |name| matcher.matches(name)) {
                    return true;
                }
                found.push((item.share, item.rel.clone(), item.stat));
                if found.len() >= self.limits.matches {
                    report.stopped = true;
                    return false;
                }
                true
            });
            match walk {
                Ok(part) => merge(&mut report, part),
                Err(err) => return Ok(fs_error(err)),
            }
            if report.stopped {
                break;
            }
        }
        found.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        let mut lines = Vec::new();
        if report.stopped {
            lines.push(format!(
                "达到搜索上限（遍历 {} 项、匹配 {} 个），后面可能还有结果，缩小范围再找。",
                budget.entries,
                found.len()
            ));
        }
        Ok(page_result(
            found,
            scope.offset,
            scope.limit,
            &report,
            None,
            lines,
            |(share, rel, stat)| link(share, rel, stat),
        ))
    }

    #[tool(
        annotations(read_only_hint = true, title = "按内容找文字文件"),
        description = "在共享文件夹里按内容找文字文件，超过大小上限和二进制文件会跳过并计数。结果分页，每页最多 100 个。跳过的条目和达到的上限会写在结果里。每个文件返回成 wonton-share 引用。要发给你时，对结果里的文件调 wonton__share_file。"
    )]
    pub async fn search_content(
        &self,
        Parameters(params): Parameters<SearchContent>,
    ) -> Result<CallToolResult, McpError> {
        let permit = match self.search_slots.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(err) => return Ok(output_error(err.to_string())),
        };
        self.spawn_tool(move |server| {
            let _permit = permit;
            server.search_content_blocking(params)
        })
        .await
    }

    fn search_content_blocking(&self, params: SearchContent) -> Result<CallToolResult, McpError> {
        if let Some(delay) = self.delay {
            std::thread::sleep(delay);
        }
        if params.query.is_empty() {
            return Ok(output_error("query 不能为空"));
        }
        let scope = match self.scope(
            params.share.as_deref(),
            params.subdir.as_deref(),
            params.modified_after.as_deref(),
            params.kind,
            params.offset,
            params.limit,
        ) {
            Ok(scope) => scope,
            Err(err) => return Ok(fs_error(err)),
        };
        let max_size = params
            .max_size
            .unwrap_or(DEFAULT_CONTENT_SIZE)
            .clamp(1, MAX_CONTENT_SIZE);

        let mut found = Vec::new();
        let mut report = WalkReport::default();
        let mut counters = ContentCounters::default();
        let mut budget = Budget::new(self.limits.entries);
        'shares: for (share, base) in &scope.shares {
            let walk = self.fs.walk(share, base, &mut budget, |item| {
                if !self.matches(item, &scope, |_| true) {
                    return true;
                }
                match self.scan_file(item, &params.query, max_size, &mut counters) {
                    Scan::Match => {
                        found.push((item.share, item.rel.clone(), item.stat));
                        if found.len() >= self.limits.matches {
                            report.stopped = true;
                            return false;
                        }
                    }
                    Scan::Skip => {}
                    Scan::Stop => {
                        report.stopped = true;
                        return false;
                    }
                }
                true
            });
            match walk {
                Ok(part) => merge(&mut report, part),
                Err(err) => return Ok(fs_error(err)),
            }
            if report.stopped {
                break 'shares;
            }
        }
        if counters.stopped {
            report.stopped = true;
        }
        found.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        let mut lines = Vec::new();
        push_note(
            &mut lines,
            &counters.too_large,
            "个文件超过大小上限，已跳过",
        );
        push_note(
            &mut lines,
            &counters.unreadable,
            "个文件打不开或读不了，已跳过",
        );
        push_note(&mut lines, &counters.binary, "个文件是二进制，已跳过");
        push_note(
            &mut lines,
            &counters.not_utf8,
            "个文件不是 UTF-8 编码，已跳过",
        );
        if report.stopped {
            lines.push(format!(
                "达到内容搜索上限（遍历 {} 项、读取 {} 个文件、{} 字节），后面可能还有结果，缩小范围再找。",
                budget.entries, counters.files, counters.bytes
            ));
        }
        Ok(page_result(
            found,
            scope.offset,
            scope.limit,
            &report,
            Some(&counters),
            lines,
            |(share, rel, stat)| link(share, rel, stat),
        ))
    }

    #[tool(
        annotations(read_only_hint = true, title = "列目录"),
        description = "列出共享文件夹里的一个目录：文件返回成 wonton-share 引用，子目录和软链接返回成文字。跳过的条目会写在结果里。结果分页，每页最多 100 项。要发给你时，对文件调 wonton__share_file。"
    )]
    pub async fn list_directory(
        &self,
        Parameters(params): Parameters<ListDirectory>,
    ) -> Result<CallToolResult, McpError> {
        self.spawn_tool(move |server| server.list_directory_blocking(params))
            .await
    }

    fn list_directory_blocking(&self, params: ListDirectory) -> Result<CallToolResult, McpError> {
        let share = match self.fs.share(&params.share) {
            Ok(share) => share,
            Err(err) => return Ok(fs_error(err)),
        };
        let base = match self.fs.split_rel(params.path.as_deref().unwrap_or("")) {
            Ok(rel) => rel,
            Err(err) => return Ok(fs_error(err)),
        };
        let dir = match self.fs.open_dir(share, &base) {
            Ok(dir) => dir,
            Err(err) => return Ok(fs_error(err)),
        };
        let entries = match rustix::fs::Dir::read_from(&dir) {
            Ok(entries) => entries,
            Err(err) => return Ok(output_error(format!("读不了目录：{err}"))),
        };
        let mut report = WalkReport::default();
        let mut items: Vec<DirItem> = Vec::new();
        let mut budget = Budget::new(self.limits.entries);
        for entry in entries {
            let Ok(entry) = entry else {
                report.stat_failed.add(display(share, &base));
                continue;
            };
            let name = OsStr::from_bytes(entry.file_name().to_bytes()).to_owned();
            if name == "." || name == ".." {
                continue;
            }
            if budget.tick() {
                report.stopped = true;
                break;
            }
            if let Some(item) = list_item(&dir, share, &base, name, &mut report) {
                items.push(item);
            }
        }
        items.sort_by(|a, b| a.0.cmp(&b.0));
        let total = items.len() as u64;
        let (start, end) = window(total, params.offset.unwrap_or(0), page_limit(params.limit));
        let mut content = vec![ContentBlock::text(format!(
            "{}：{total} 项",
            display(share, &base)
        ))];
        let mut structured_items = Vec::new();
        for (name, file) in &items[start..end] {
            match file {
                Some((rel, stat)) => {
                    content.push(link(share, rel, stat));
                    structured_items.push(json!({
                        "name": name.to_string_lossy(),
                        "kind": "file",
                        "uri": link_uri(share, rel),
                    }));
                }
                None => {
                    let name = name.to_string_lossy().into_owned();
                    let kind = if name.ends_with('/') { "dir" } else { "other" };
                    content.push(ContentBlock::text(name.clone()));
                    structured_items.push(json!({ "name": name, "kind": kind }));
                }
            }
        }
        if total == 0 {
            content.push(ContentBlock::text("空目录".to_owned()));
        }
        let next = (end < total as usize).then_some(end as u64);
        if let Some(next) = next {
            content.push(ContentBlock::text(format!(
                "后面还有 {} 项，用 offset={next} 继续。",
                total as usize - end
            )));
        }
        if report.stopped {
            content.push(ContentBlock::text(format!(
                "达到列目录上限（{} 项），后面还有条目，缩小 path 再列。",
                budget.entries
            )));
        }
        for line in report_lines(&report) {
            content.push(ContentBlock::text(line));
        }
        Ok(crate::output::success(
            content,
            json!({
                "total": total,
                "offset": start,
                "returned": end - start,
                "next_offset": next,
                "stopped": report.stopped,
                "items": structured_items,
                "notes": report_value(&report),
            }),
        ))
    }

    #[tool(
        annotations(read_only_hint = true, title = "读文字文件"),
        description = "读共享文件夹里一个文字文件的内容。默认最多 64 KiB，超过就分段：结果末尾给出下一个 offset。二进制文件报错。要发给你时，对文件调 wonton__share_file。"
    )]
    pub async fn read_text_file(
        &self,
        Parameters(params): Parameters<ReadTextFile>,
    ) -> Result<CallToolResult, McpError> {
        self.spawn_tool(move |server| server.read_text_file_blocking(params))
            .await
    }

    fn read_text_file_blocking(&self, params: ReadTextFile) -> Result<CallToolResult, McpError> {
        let share = match self.fs.share(&params.share) {
            Ok(share) => share,
            Err(err) => return Ok(fs_error(err)),
        };
        let rel = match self.fs.split_rel(&params.path) {
            Ok(rel) => rel,
            Err(err) => return Ok(fs_error(err)),
        };
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(MAX_READ).clamp(1, MAX_READ);
        let (bytes, size, start) = match self.fs.read_range(share, &rel, offset, limit) {
            Ok(read) => read,
            Err(err) => return Ok(fs_error(err)),
        };
        let (skip, text) =
            match crate::text::read_chunk(&bytes, (offset - start) as usize, limit as usize) {
                Ok(chunk) => chunk,
                Err(err) => return Ok(fs_error(err)),
            };
        let base = offset + skip as u64;
        let end = text.len();
        let next = (base + (end as u64) < size).then_some(base + end as u64);
        Ok(read_result(
            share,
            &rel,
            size,
            offset,
            skip as u64,
            text,
            next,
        ))
    }

    fn scan_file(
        &self,
        item: &Found,
        query: &str,
        max_size: u64,
        counters: &mut ContentCounters,
    ) -> Scan {
        if counters.files >= self.limits.content_files
            || counters.bytes >= self.limits.content_bytes
        {
            counters.stopped = true;
            return Scan::Stop;
        }
        let here = display(item.share, &item.rel);
        let size = item.stat.st_size.max(0) as u64;
        if size > max_size {
            counters
                .too_large
                .add(format!("{here}（{}）", FsError::TooLarge(size)));
            return Scan::Skip;
        }
        let Ok((file, _)) = self.fs.open_file(item.share, &item.rel) else {
            counters.unreadable.add(here);
            return Scan::Skip;
        };
        let mut data = Vec::new();
        if file.take(max_size + 1).read_to_end(&mut data).is_err() {
            counters.unreadable.add(here);
            return Scan::Skip;
        }
        counters.files += 1;
        counters.bytes += data.len() as u64;
        if data.len() as u64 > max_size {
            counters.too_large.add(format!(
                "{here}（{}）",
                FsError::TooLarge(data.len() as u64)
            ));
            return Scan::Skip;
        }
        match classify_text(&data) {
            TextKind::Binary => {
                counters.binary.add(here);
                return Scan::Skip;
            }
            TextKind::NotUtf8 => {
                counters.not_utf8.add(here);
                return Scan::Skip;
            }
            TextKind::Text => {}
        }
        if data
            .windows(query.len())
            .any(|window| window == query.as_bytes())
        {
            Scan::Match
        } else {
            Scan::Skip
        }
    }

    async fn spawn_tool<F>(&self, work: F) -> Result<CallToolResult, McpError>
    where
        F: FnOnce(FilesServer) -> Result<CallToolResult, McpError> + Send + 'static,
    {
        let server = self.clone();
        match tokio::task::spawn_blocking(move || work(server)).await {
            Ok(result) => result,
            Err(err) => Ok(output_error(format!("文件操作线程失败：{err}"))),
        }
    }

    fn scope(
        &self,
        share: Option<&str>,
        subdir: Option<&str>,
        modified_after: Option<&str>,
        kind: Option<KindParam>,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> Result<Scope, FsError> {
        let base = match subdir {
            Some(path) => self.fs.split_rel(path)?,
            None => Vec::new(),
        };
        let shares = if share.is_none() && !base.is_empty() {
            return Err(FsError::InvalidParam("subdir 要和 share 一起用".to_owned()));
        } else {
            match share {
                Some(share) => vec![(self.fs.share(share)?, base)],
                None => SHARES.iter().map(|share| (*share, Vec::new())).collect(),
            }
        };
        let after = match modified_after {
            Some(text) => Some(parse_time(text)?),
            None => None,
        };
        Ok(Scope {
            shares,
            after,
            kind: kind.map(Into::into),
            offset: offset.unwrap_or(0),
            limit: page_limit(limit),
        })
    }

    fn matches(&self, item: &Found, scope: &Scope, name_matches: impl Fn(&str) -> bool) -> bool {
        if let Some(after) = scope.after
            && item.stat.st_mtime < after
        {
            return false;
        }
        let name = file_name(&item.rel);
        if let Some(kind) = scope.kind
            && kind_of(&name) != kind
        {
            return false;
        }
        name_matches(&name)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for FilesServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(rmcp::model::Implementation::new(
                "wonton-files",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "只读的文件搜索。找到的文件是 wonton-share: 引用，要发给你就对它调 wonton__share_file。",
            )
    }
}

fn output_error(message: impl Into<String>) -> CallToolResult {
    crate::output::error("invalid_param", message)
}
