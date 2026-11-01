//! Leave as HR runs it: one project per kind of leave, a parent issue per
//! month in each ("Xin nghỉ phép Tháng 10.2026"), and one sub-task per day
//! off ("BE - Tỷ xin nghỉ phép ngày 29/10/2026").

/// What a project holds, from its name: (key, label, verb for the subject).
/// `None` when the name does not look like a leave project.
pub fn classify_project(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    let n = name.to_lowercase();
    if !(n.contains("leave") || n.contains("nghỉ")) {
        return None;
    }
    Some(if n.contains("unpaid") || n.contains("không lương") {
        ("unpaid", "Nghỉ không lương", "xin nghỉ không lương")
    } else if n.contains("sick") || n.contains("đột xuất") || n.contains("ốm") {
        ("sick", "Nghỉ đột xuất", "xin nghỉ đột xuất")
    } else if n.contains("annual") || n.contains("phép năm") || n.contains("nghỉ phép") {
        ("annual", "Nghỉ phép năm", "xin nghỉ phép")
    } else {
        ("other", "Nghỉ khác", "xin nghỉ")
    })
}

/// True when `subject` names the month, as "10.2026", "T10.2026",
/// "Tháng 10/2026" or "10-2026" (leading zero optional).
pub fn mentions_month(subject: &str, year: i32, month: u32) -> bool {
    let chars: Vec<char> = subject.chars().collect();
    let year = year.to_string();
    for (i, _) in chars.iter().enumerate() {
        // A month number starts here only if no digit precedes it.
        if i > 0 && chars[i - 1].is_ascii_digit() {
            continue;
        }
        let digits: String = chars[i..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if digits.is_empty() || digits.len() > 2 || digits.parse::<u32>().ok() != Some(month) {
            continue;
        }
        let rest: String = chars[i + digits.len()..].iter().collect();
        let mut rest = rest.chars();
        if !matches!(rest.next(), Some('.' | '/' | '-')) {
            continue;
        }
        let tail: String = rest.collect();
        if tail.starts_with(&year) && !tail[year.len()..].starts_with(|c: char| c.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

/// The part of a leave issue's subject before the leave itself, which is how
/// the person names themselves: "BE - Tỷ off đột xuất ngày 19/10" → "BE - Tỷ".
pub fn name_from_subject(subject: &str) -> Option<String> {
    let chars: Vec<char> = subject.chars().collect();
    // Lowercase char by char so positions stay aligned with `chars`.
    let lower: Vec<char> = chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let lower: String = lower.into_iter().collect();
    let cut = [" xin ", " off ", " nghỉ ", "-nghỉ ", "- nghỉ "]
        .iter()
        .filter_map(|m| lower.find(m).map(|b| lower[..b].chars().count()))
        .min()?;
    let name: String = chars[..cut].iter().collect();
    let name = name.trim().trim_end_matches(['-', ':']).trim();
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_month_in_hr_subjects() {
        assert!(mentions_month("Xin nghỉ phép Tháng 10.2026", 2026, 10));
        assert!(mentions_month(
            "Xin nghỉ phép Đột Xuất Tháng 09.2026",
            2026,
            9
        ));
        assert!(mentions_month(
            "Xin nghỉ phép Không Lương T04.2026",
            2026,
            4
        ));
        assert!(mentions_month("Xin Nghỉ Phép Năm Tháng 05.2026", 2026, 5));
        assert!(mentions_month("Nghỉ phép 5/2026", 2026, 5));
        assert!(!mentions_month("Xin nghỉ phép Tháng 11.2026", 2026, 1));
        assert!(!mentions_month("Xin nghỉ phép Tháng 10.2025", 2026, 10));
        assert!(!mentions_month("Xin nghỉ phép Tháng 10.20261", 2026, 10));
        assert!(!mentions_month("Estuary Townhall Q3", 2026, 10));
    }

    #[test]
    fn learns_the_name_prefix() {
        let n = |s| name_from_subject(s);
        assert_eq!(
            n("BE - Tỷ off đột xuất ngày 19/10").as_deref(),
            Some("BE - Tỷ")
        );
        assert_eq!(
            n("CS - Thảo Quyên xin nghỉ phép ngày 01 - 02/10").as_deref(),
            Some("CS - Thảo Quyên")
        );
        assert_eq!(
            n("DevOps: Hồ Tấn Lộc xin nghỉ phép không lương").as_deref(),
            Some("DevOps: Hồ Tấn Lộc")
        );
        assert_eq!(
            n("PS-Minh Hạnh-Nghỉ phép ngày 29/10/2026").as_deref(),
            Some("PS-Minh Hạnh")
        );
        assert_eq!(
            n("TL - Minh Nhật nghỉ phép 05-06/10").as_deref(),
            Some("TL - Minh Nhật")
        );
        assert_eq!(n("Estuary Townhall Q3"), None);
    }

    #[test]
    fn classifies_leave_projects() {
        assert_eq!(classify_project("Annual Leave").unwrap().0, "annual");
        assert_eq!(classify_project("Sick Leave").unwrap().0, "sick");
        assert_eq!(classify_project("Unpaid Leave").unwrap().0, "unpaid");
        assert!(classify_project("Public Holiday").is_none());
        assert!(classify_project("Internal Meeting").is_none());
    }
}
