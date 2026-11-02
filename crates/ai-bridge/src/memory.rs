//! What the user chose before: report wording → parent task and sub-task.
//!
//! Reports repeat the same work day after day, so each confirmed write is
//! remembered and used three ways: a task Claude finds again gets the
//! user's own choice, Claude is told about known work so it searches less,
//! and "Khớp nhanh" matches a report against it without Claude at all.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::report::{DayWeight, ReportAnalysis, ReportTask, tidy_report};

/// Most entries kept; the least recently used go first.
const LIMIT: usize = 500;
/// Known tasks shown to Claude.
const PROMPT_LIMIT: usize = 60;
/// Progress words that do not change which task a line is about.
const STATUS: &[&str] = &["wip", "done", "xong", "tiếp", "tục", "đang", "đã"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Remembered {
    /// The report line as the user wrote it.
    pub text: String,
    pub parent_id: u32,
    pub parent_subject: String,
    /// The sub-task the time went to (created by the write, or chosen).
    pub subtask_id: Option<u32>,
    pub subtask_subject: String,
    #[serde(default)]
    pub project_hint: String,
    #[serde(default)]
    pub comment: String,
    pub uses: u32,
    pub last_used: NaiveDate,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Memory {
    pub entries: Vec<Remembered>,
}

/// The part of a line that says which work it is: no bullets, numbering,
/// brackets, percentages or progress words; lower case.
pub fn key(text: &str) -> String {
    let mut plain = String::new();
    let mut depth = 0usize;
    for c in strip_marker(text.trim()).chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => plain.push(c),
            _ => {}
        }
    }
    plain
        .split_whitespace()
        .filter(|w| !w.ends_with('%'))
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| !w.is_empty() && !STATUS.contains(&w.as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// "- a", "+ a", "• a", "1. a", "2) a" → "a".
fn strip_marker(line: &str) -> &str {
    let line = line
        .trim_start_matches(['-', '+', '*', '•', '·', '–'])
        .trim_start();
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits <= 2 {
        let rest = &line[digits..];
        if let Some(r) = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')')) {
            if r.starts_with(char::is_whitespace) {
                return r.trim_start();
            }
        }
    }
    line
}

impl Memory {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Adds or refreshes what the user chose for a line.
    pub fn learn(&mut self, mut new: Remembered) {
        let k = key(&new.text);
        if k.is_empty() || new.parent_id == 0 {
            return;
        }
        if let Some(old) = self.entries.iter_mut().find(|e| key(&e.text) == k) {
            new.uses = old.uses + 1;
            *old = new;
        } else {
            new.uses = 1;
            self.entries.push(new);
        }
        self.entries
            .sort_by(|a, b| b.last_used.cmp(&a.last_used).then(b.uses.cmp(&a.uses)));
        self.entries.truncate(LIMIT);
    }

    /// The remembered task for a line, by its wording or the sub-task's name.
    pub fn find(&self, text: &str) -> Option<&Remembered> {
        let k = key(text);
        if k.is_empty() {
            return None;
        }
        self.entries
            .iter()
            .find(|e| key(&e.text) == k || key(&e.subtask_subject) == k)
    }

    /// Gives tasks the user has logged before their earlier parent and
    /// sub-task. Returns how many were settled this way.
    pub fn apply(&self, analysis: &mut ReportAnalysis) -> usize {
        let mut n = 0;
        for t in &mut analysis.tasks {
            let Some(e) = self.find(&t.text).or_else(|| self.find(&t.subtask_subject)) else {
                continue;
            };
            t.parent_issue_id = Some(e.parent_id);
            t.existing_subtask_id = e.subtask_id;
            t.subtask_subject = e.subtask_subject.clone();
            t.parent_confidence = 1.0;
            t.parent_reason = REASON.into();
            n += 1;
        }
        n
    }

    /// Known work for the prompt, most recent first.
    pub fn prompt_hint(&self) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        let lines: Vec<String> = self
            .entries
            .iter()
            .take(PROMPT_LIMIT)
            .map(|e| {
                let sub = e
                    .subtask_id
                    .map(|id| format!(", own sub-task #{id} “{}”", e.subtask_subject))
                    .unwrap_or_default();
                format!(
                    "- “{}” → parent #{} “{}”{sub}",
                    e.text, e.parent_id, e.parent_subject
                )
            })
            .collect();
        Some(lines.join("\n"))
    }
}

const REASON: &str = "Lần trước bạn đã chọn task này.";

/// Worth reporting as unknown work: not a numbered project header, a
/// heading that ends with ":", or chat around the report.
fn looks_like_work(line: &str) -> bool {
    let t = line.trim();
    let lower = format!(" {} ", t.to_lowercase());
    let numbered_header =
        t.chars().next().is_some_and(|c| c.is_ascii_digit()) && strip_marker(t) != t;
    let chat = [
        "báo cáo",
        " dạ ",
        " ạ ",
        "kế hoạch",
        "@",
        " hi ",
        "chào",
        "cảm ơn",
    ]
    .iter()
    .any(|w| lower.contains(w));
    !(numbered_header || t.ends_with(':') || chat)
}

/// Day/month(/year) dates in a line, e.g. "27/10/2026" or "27/10".
fn dates_in(line: &str, year: i32) -> Vec<NaiveDate> {
    line.split(|c: char| !(c.is_ascii_digit() || c == '/'))
        .filter_map(|w| {
            let parts: Vec<&str> = w.split('/').filter(|p| !p.is_empty()).collect();
            let num = |s: &str| s.parse::<u32>().ok();
            match parts.as_slice() {
                [d, m] => NaiveDate::from_ymd_opt(year, num(m)?, num(d)?),
                [d, m, y] if y.len() == 4 => {
                    NaiveDate::from_ymd_opt(y.parse().ok()?, num(m)?, num(d)?)
                }
                _ => None,
            }
        })
        .collect()
}

/// A day/month without a year belongs to the year that keeps it closest
/// before `near`.
fn in_year_near(date: NaiveDate, near: NaiveDate) -> NaiveDate {
    if date > near + chrono::Duration::days(31) {
        date.with_year(date.year() - 1).unwrap_or(date)
    } else {
        date
    }
}

/// "Khớp nhanh": the report matched against remembered tasks, no Claude.
/// Dates come from day headers ("… 27/10/2026", "Thứ 2 26/10:") or the send
/// time of chat messages; ranges are plans and are skipped. Lines that match
/// nothing remembered are listed in `questions`. `day` pins every line to
/// that date (a calendar day's report).
pub fn quick_analysis(
    memory: &Memory,
    text: &str,
    today: NaiveDate,
    day: Option<NaiveDate>,
) -> Result<ReportAnalysis, String> {
    if memory.is_empty() {
        return Err("Chưa có việc nào được ghi nhớ. Phân tích bằng AI và ghi lần đầu, app sẽ nhớ cho các lần sau.".into());
    }
    let mut current = day;
    let mut sent: Option<NaiveDate> = None;
    let mut in_plan = false;
    // Remembered entry index → dates it was done on.
    let mut found: BTreeMap<usize, Vec<NaiveDate>> = BTreeMap::new();
    let mut unknown: Vec<String> = Vec::new();
    let mut questions: Vec<String> = Vec::new();

    for raw in tidy_report(text).lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("[Gửi lúc ") {
            sent = dates_in(line, today.year()).into_iter().next();
            current = day.or(sent);
            in_plan = false;
            continue;
        }
        let near = sent.unwrap_or(today);
        let dates: Vec<NaiveDate> = dates_in(line, near.year())
            .into_iter()
            .map(|d| in_year_near(d, near))
            .collect();
        let mut rest = line;
        if dates.len() >= 2 {
            if day.is_none() {
                in_plan = true;
                questions.push(format!(
                    "Khớp nhanh bỏ qua kế hoạch “{line}”, chỉ đọc báo cáo theo ngày."
                ));
            }
            continue;
        }
        if let [date] = dates.as_slice() {
            in_plan = false;
            if day.is_none() {
                current = Some(*date);
            }
            // "26/10: Sửa feedback" carries work after the date.
            match line.split_once(':') {
                Some((_, after)) if !after.trim().is_empty() => rest = after,
                _ => continue,
            }
        }
        if in_plan {
            continue;
        }
        match memory.entries.iter().position(|e| {
            let k = key(rest);
            !k.is_empty() && (key(&e.text) == k || key(&e.subtask_subject) == k)
        }) {
            Some(i) => match current {
                Some(d) if d <= today || day.is_some() => {
                    let days = found.entry(i).or_default();
                    if !days.contains(&d) {
                        days.push(d);
                    }
                }
                Some(_) => {}
                None => questions.push(format!("Không rõ ngày của “{}”.", rest.trim())),
            },
            None => {
                let words = key(rest).split_whitespace().count();
                if rest.to_lowercase().contains("nghỉ") {
                    questions.push(format!(
                        "Khớp nhanh không ghi nghỉ phép: “{}”. Dùng Xin nghỉ trên Lịch tháng.",
                        rest.trim()
                    ));
                } else if words >= 2 && looks_like_work(rest) {
                    unknown.push(rest.trim().to_string());
                }
            }
        }
    }

    if !unknown.is_empty() {
        let shown: Vec<String> = unknown.iter().take(5).map(|l| format!("“{l}”")).collect();
        let more = unknown.len().saturating_sub(5);
        questions.push(format!(
            "Chưa nhận ra {} dòng (chưa ghi lần nào): {}{}. Dùng Phân tích bằng AI cho các việc mới.",
            unknown.len(),
            shown.join(", "),
            if more > 0 { format!(" và {more} dòng khác") } else { String::new() }
        ));
    }
    let tasks: Vec<ReportTask> = found
        .into_iter()
        .map(|(i, mut days)| {
            days.sort();
            let e = &memory.entries[i];
            ReportTask {
                key: format!("m{i}"),
                text: e.text.clone(),
                project_hint: e.project_hint.clone(),
                parent_issue_id: Some(e.parent_id),
                parent_confidence: 1.0,
                parent_reason: REASON.into(),
                existing_subtask_id: e.subtask_id,
                subtask_subject: e.subtask_subject.clone(),
                comment: if e.comment.is_empty() {
                    e.text.clone()
                } else {
                    e.comment.clone()
                },
                days: days
                    .into_iter()
                    .map(|date| DayWeight { date, weight: 1.0 })
                    .collect(),
            }
        })
        .collect();
    let all = tasks.iter().flat_map(|t| t.days.iter().map(|d| d.date));
    let (Some(from), Some(to)) = (all.clone().min(), all.max()) else {
        return Err(format!(
            "Không khớp được việc nào với {} việc đã nhớ. Dùng Phân tích bằng AI.{}",
            memory.len(),
            questions
                .last()
                .map(|q| format!(" {q}"))
                .unwrap_or_default()
        ));
    };
    Ok(ReportAnalysis {
        period_from: from,
        period_to: to,
        leave: Vec::new(),
        questions,
        tasks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    fn mem(text: &str, parent: u32, sub: Option<u32>) -> Remembered {
        Remembered {
            text: text.into(),
            parent_id: parent,
            parent_subject: format!("Parent {parent}"),
            subtask_id: sub,
            subtask_subject: format!("[FE] {text}"),
            project_hint: "ABC".into(),
            comment: String::new(),
            uses: 0,
            last_used: d(20),
        }
    }

    fn memory() -> Memory {
        let mut m = Memory::default();
        m.learn(mem("Sửa feedback công nợ", 1, Some(4)));
        m.learn(mem("Review Module Sự kiện", 2, Some(5)));
        m
    }

    #[test]
    fn key_ignores_bullets_numbers_brackets_and_progress() {
        assert_eq!(
            key("- Sửa feedback công nợ (filter + export) 50%"),
            "sửa feedback công nợ"
        );
        assert_eq!(
            key("2) Tiếp tục sửa feedback công nợ: xong"),
            "sửa feedback công nợ"
        );
        assert_eq!(key("1. Davipharm - Salex"), "davipharm salex");
    }

    #[test]
    fn learn_refreshes_the_same_line_and_counts_uses() {
        let mut m = memory();
        let mut again = mem("sửa FEEDBACK công nợ", 9, None);
        again.last_used = d(28);
        m.learn(again);
        assert_eq!(m.len(), 2);
        assert_eq!(m.entries[0].parent_id, 9);
        assert_eq!(m.entries[0].uses, 2);
        // Lines without a parent or without words are not remembered.
        m.learn(mem("  -  ", 3, None));
        m.learn(mem("Họp", 0, None));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn apply_gives_known_tasks_the_earlier_choice() {
        let m = memory();
        let task = |text: &str| ReportTask {
            key: text.into(),
            text: text.into(),
            project_hint: String::new(),
            parent_issue_id: Some(77),
            parent_confidence: 0.4,
            parent_reason: "đoán".into(),
            existing_subtask_id: None,
            subtask_subject: String::new(),
            comment: String::new(),
            days: vec![],
        };
        let mut a = ReportAnalysis {
            period_from: d(27),
            period_to: d(27),
            leave: vec![],
            questions: vec![],
            tasks: vec![task("Review module sự kiện (xong)"), task("Việc mới")],
        };
        assert_eq!(m.apply(&mut a), 1);
        assert_eq!(a.tasks[0].parent_issue_id, Some(2));
        assert_eq!(a.tasks[0].existing_subtask_id, Some(5));
        assert_eq!(a.tasks[0].parent_confidence, 1.0);
        assert_eq!(a.tasks[1].parent_issue_id, Some(77));
    }

    #[test]
    fn quick_analysis_reads_day_headers_and_chat_send_times() {
        let text = "[Gửi lúc Thứ ba 27/10/2026 09:23]
Dạ em xin gửi báo cáo công việc tuần này 27/10/2026 ạ
1. Dự án ABC
- Sửa feedback công nợ
- Viết tài liệu mới hoàn toàn

[Gửi lúc Thứ tư 28/10/2026 17:40]
Báo cáo hôm nay:
- Tiếp tục sửa feedback công nợ
- Review Module Sự kiện (50%)
Chiều mai em xin nghỉ ạ

[Gửi lúc Thứ năm 29/10/2026 08:10]
Kế hoạch 26/10/2026 - 30/10/2026
- Review Module Sự kiện
Thứ 5 29/10:
- Review module sự kiện";
        let a = quick_analysis(&memory(), text, d(29), None).unwrap();
        let days = |key: &str| {
            a.tasks
                .iter()
                .find(|t| t.text.contains(key))
                .unwrap()
                .days
                .iter()
                .map(|x| x.date)
                .collect::<Vec<_>>()
        };
        assert_eq!(days("công nợ"), vec![d(27), d(28)]);
        // The weekly plan line is skipped; the day header after it counts.
        assert_eq!(days("Sự kiện"), vec![d(28), d(29)]);
        assert_eq!((a.period_from, a.period_to), (d(27), d(29)));
        assert_eq!(a.tasks[0].parent_reason, REASON);
        let q = a.questions.join(" | ");
        assert!(q.contains("Viết tài liệu mới hoàn toàn"), "{q}");
        assert!(q.contains("nghỉ"), "{q}");
        assert!(q.contains("kế hoạch"), "{q}");
        // Headers and greetings are not reported as unknown work.
        assert!(!q.contains("Dự án ABC") && !q.contains("Dạ em"), "{q}");
    }

    #[test]
    fn quick_analysis_pins_a_calendar_day_and_skips_future_dates() {
        let a = quick_analysis(
            &memory(),
            "Sửa feedback công nợ\n- Review Module Sự kiện",
            d(29),
            Some(d(26)),
        )
        .unwrap();
        assert!(
            a.tasks
                .iter()
                .all(|t| t.days.len() == 1 && t.days[0].date == d(26))
        );
        let err = quick_analysis(&memory(), "30/10/2026\n- Sửa feedback công nợ", d(29), None)
            .unwrap_err();
        assert!(err.contains("Không khớp"), "{err}");
        assert!(quick_analysis(&Memory::default(), "x", d(29), None).is_err());
    }
}
