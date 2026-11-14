//! 遍历时发现的问题计数和样本，以及渲染成文字。

use serde_json::{Value, json};

const SAMPLES: usize = 5;

#[derive(Debug, Default)]
pub struct Notes {
    pub count: usize,
    pub samples: Vec<String>,
}

impl Notes {
    pub fn add(&mut self, path: String) {
        self.count += 1;
        if self.samples.len() < SAMPLES {
            self.samples.push(path);
        }
    }
}

#[derive(Debug, Default)]
pub struct WalkReport {
    pub non_utf8: Notes,
    pub too_deep: Notes,
    pub denied: Notes,
    pub stat_failed: Notes,
    pub stopped: bool,
}

pub fn merge(report: &mut WalkReport, part: WalkReport) {
    merge_notes(&mut report.non_utf8, part.non_utf8);
    merge_notes(&mut report.too_deep, part.too_deep);
    merge_notes(&mut report.denied, part.denied);
    merge_notes(&mut report.stat_failed, part.stat_failed);
    report.stopped |= part.stopped;
}

fn merge_notes(into: &mut Notes, from: Notes) {
    for sample in from.samples {
        if into.samples.len() < SAMPLES {
            into.samples.push(sample);
        }
    }
    into.count += from.count;
}

pub fn note_value(note: &Notes) -> Value {
    json!({ "count": note.count, "samples": note.samples })
}

pub fn push_note(lines: &mut Vec<String>, note: &Notes, label: &str) {
    if let Some(line) = note_line(note, label) {
        lines.push(line);
    }
}

fn note_line(note: &Notes, label: &str) -> Option<String> {
    if note.count == 0 {
        return None;
    }
    let mut line = format!("{} {label}", note.count);
    if !note.samples.is_empty() {
        line.push('：');
        line.push_str(
            &note
                .samples
                .iter()
                .map(|sample| short(sample))
                .collect::<Vec<_>>()
                .join("、"),
        );
        if note.count > note.samples.len() {
            line.push_str(" …");
        }
    }
    Some(line)
}

/// 样本路径太长时保留首尾，中间用 `…`。
fn short(sample: &str) -> String {
    const EDGE: usize = 30;
    if sample.chars().count() <= 80 {
        return sample.to_owned();
    }
    let head: String = sample.chars().take(EDGE).collect();
    let tail: String = sample
        .chars()
        .rev()
        .take(EDGE)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head}…{tail}")
}

pub fn report_lines(report: &WalkReport) -> Vec<String> {
    let mut lines = Vec::new();
    push_note(
        &mut lines,
        &report.non_utf8,
        "个文件名不是 UTF-8，已按百分号编码",
    );
    push_note(&mut lines, &report.too_deep, "个目录超过 64 层，没有进入");
    push_note(&mut lines, &report.denied, "个目录或文件没有权限，已跳过");
    push_note(&mut lines, &report.stat_failed, "项 stat 失败，已跳过");
    lines
}

pub fn report_value(report: &WalkReport) -> Value {
    json!({
        "non_utf8": note_value(&report.non_utf8),
        "too_deep": note_value(&report.too_deep),
        "denied": note_value(&report.denied),
        "stat_failed": note_value(&report.stat_failed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_samples_are_shortened_in_the_middle() {
        let path = format!("documents/{}d64", "d00/".repeat(30));
        let shortened = short(&path);
        assert!(shortened.starts_with("documents/"));
        assert!(shortened.ends_with("d64"));
        assert_eq!(shortened.chars().count(), 61);
        let mut note = Notes::default();
        note.add(path.clone());
        assert_eq!(note_value(&note)["count"], 1);
        assert_eq!(note_value(&note)["samples"][0], path);
    }

    #[test]
    fn report_value_has_each_reason() {
        let mut report = WalkReport::default();
        report.denied.add("documents/secret".to_owned());
        let value = report_value(&report);
        assert_eq!(value["denied"]["count"], 1);
        assert_eq!(value["too_deep"]["count"], 0);
    }
}
