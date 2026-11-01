//! Runs the real Claude CLI + mcp-redmine against the local Docker Redmine:
//! structure discovery, then analysis of a sample report.
//!
//!   REDMINE_API_KEY=<dev1 key> cargo run -p ai-bridge --example local_ai

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use ai_bridge::profile::{self, Profile};
use ai_bridge::report::{self, DayCapacity, ReportAnalysis, ReportContext};
use ai_bridge::{ClaudeRunner, RedmineMcp};
use chrono::NaiveDate;

const REPORT: &str = "Dạ em xin gửi báo cáo công việc tuần này 26/10/2026 ạ
1. Dự án ABC
Sửa feedback công nợ
Review Module Sự kiện

Thứ 3 27/10:
- Sửa feedback công nợ
Thứ 4 28/10: nghỉ phép buổi chiều
- Review Module Sự kiện";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let work = std::env::temp_dir().join("local-ai-example");
    std::fs::create_dir_all(&work)?;
    let runner = ClaudeRunner {
        program: "claude".into(),
        path_env: std::env::var("PATH")?,
        workdir: work.clone(),
        timeout: Duration::from_secs(600),
        ignore_user_settings: true,
    };
    let instructions = work.join("profile.md");
    let mcp = RedmineMcp {
        url: "http://localhost:3000".into(),
        api_key: std::env::var("REDMINE_API_KEY")?,
        accept_invalid_certs: false,
        instructions: Some(instructions.clone()),
    };
    let cancel = AtomicBool::new(false);
    let mut log = |p: ai_bridge::Progress| println!("  · {}", p.text);

    println!("== discovery");
    let prof: Profile = runner
        .run(
            &profile::discovery_prompt(),
            &profile::schema(),
            Some(&mcp),
            &cancel,
            &mut log,
        )
        .await?;
    let md = profile::to_instructions(&prof);
    std::fs::write(&instructions, &md)?;
    println!("{md}");

    println!("== report");
    let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
    let prompt = report::prompt(
        REPORT,
        &ReportContext {
            today,
            hours_per_day: 8.0,
            workdays: &[1, 2, 3, 4, 5],
            profile: Some(&md),
            day: None,
            known_tasks: None,
        },
    );
    let analysis: ReportAnalysis = runner
        .run(&prompt, &report::schema(), Some(&mcp), &cancel, &mut log)
        .await?;
    println!("{}", serde_json::to_string_pretty(&analysis)?);

    let days: Vec<DayCapacity> = (26..=28)
        .map(|d| {
            let date = NaiveDate::from_ymd_opt(2026, 10, d).unwrap();
            let leave: f64 = analysis
                .leave
                .iter()
                .filter(|l| l.date == date)
                .map(|l| l.hours)
                .sum();
            DayCapacity {
                date,
                workday: true,
                existing_hours: 0.0,
                leave_hours: leave,
                capacity: 8.0 - leave,
            }
        })
        .collect();
    for p in report::plan(&analysis, &days, &[]) {
        let parts: Vec<String> = p
            .allocations
            .iter()
            .map(|a| format!("{} {}h", a.task, a.hours))
            .collect();
        println!(
            "{} leave={} -> {} {:?}",
            p.date,
            p.leave_hours,
            parts.join(", "),
            p.warnings
        );
    }
    Ok(())
}
