//! End-to-end check of the write engine against the local Docker Redmine
//! (docker/). Refuses to run against anything but localhost.
//!
//!   REDMINE_API_KEY=<dev1 key> cargo run -p logtime-engine --example local_e2e

use chrono::NaiveDate;
use logtime_engine::model::{ItemKind, OpStatus, PlanItem, SubtaskDraft, Target, WritePlan};
use logtime_engine::{Store, exec, plan};
use redmine_client::{ClientOptions, RedmineClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("REDMINE_URL").unwrap_or_else(|_| "http://localhost:3000".into());
    assert!(
        url.starts_with("http://localhost"),
        "chỉ chạy với Redmine local"
    );
    let client = RedmineClient::new(
        &url,
        &std::env::var("REDMINE_API_KEY")?,
        ClientOptions::default(),
    )?;
    let me = client.current_user().await?;

    let task = client
        .trackers()
        .await?
        .into_iter()
        .find(|t| t.name == "Task")
        .ok_or("no Task tracker")?;
    let parent = client.issue(1).await?;
    let activity = client
        .time_entry_activities()
        .await?
        .into_iter()
        .find(|a| a.name == "Development")
        .ok_or("no activity")?;
    let date = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
    let key = format!("e2e-{}", std::process::id());

    let write = WritePlan {
        subtasks: vec![SubtaskDraft {
            key: key.clone(),
            parent_id: parent.id,
            parent_subject: parent.subject.clone(),
            project_id: parent.project.id,
            project_name: parent.project.name.clone(),
            tracker_id: task.id,
            subject: format!("E2E sửa feedback công nợ {key}"),
            start_date: None,
        }],
        items: vec![PlanItem {
            id: "i1".into(),
            date,
            target: Target::NewSubtask { key },
            hours: 2.5,
            activity_id: Some(activity.id),
            comments: "e2e".into(),
            kind: ItemKind::Work,
        }],
    };

    let store = Store::open_in_memory()?;
    let ops = plan::build_ops(&write)?;
    let id = store.create_batch("e2e", &url, &ops)?;
    let batch = exec::run_batch(&store, &client, id, &url, me.id, &mut |op| {
        println!("  {:?} -> {:?} {:?}", op.status, op.result_id, op.error)
    })
    .await?;
    assert!(
        batch.ops.iter().all(|o| o.status == OpStatus::Done),
        "{batch:#?}"
    );

    let sub = client.issue(batch.ops[0].result_id.unwrap()).await?;
    println!(
        "sub-task #{} parent={:?} tracker={}",
        sub.id, sub.parent, sub.tracker.name
    );
    assert_eq!(sub.parent.map(|p| p.id), Some(parent.id));

    let before = client.my_time_entries(date, date).await?.len();
    exec::run_batch(&store, &client, id, &url, me.id, &mut |_| {}).await?;
    assert_eq!(
        client.my_time_entries(date, date).await?.len(),
        before,
        "re-run must not duplicate"
    );

    let undone = exec::undo_batch(&store, &client, id, &url, &mut |_| {}).await?;
    assert_eq!(
        undone.ops[1].status,
        OpStatus::Undone,
        "{:?}",
        undone.ops[1].error
    );
    assert_eq!(client.my_time_entries(date, date).await?.len(), before - 1);
    println!("OK: tạo task con, ghi 2.5h, chạy lại không trùng, hoàn tác xoá entry");

    // Redmine saved the entry but the answer was lost (timeout, 502...):
    // the retry must adopt it, not log the same hours twice.
    let entry = |id: &str, hours: f64, activity_id: u32| PlanItem {
        id: id.into(),
        date,
        target: Target::Issue {
            issue_id: parent.id,
            label: format!("#{}", parent.id),
        },
        hours,
        activity_id: Some(activity_id),
        comments: format!("e2e-retry-{}", std::process::id()),
        kind: ItemKind::Work,
    };
    let lost = WritePlan {
        subtasks: vec![],
        items: vec![entry("r1", 1.25, activity.id)],
    };
    let id = store.create_batch("e2e-retry", &url, &plan::build_ops(&lost)?)?;
    let saved = client
        .create_time_entry(&redmine_client::NewTimeEntry {
            issue_id: parent.id,
            spent_on: date,
            hours: 1.25,
            activity_id: Some(activity.id),
            comments: lost.items[0].comments.clone(),
        })
        .await?;
    let op = store.batch(id)?.ops[0].id;
    store.set_op(op, OpStatus::Failed, None, Some("Lỗi kết nối: timeout"))?;
    store.mark_uncertain(op)?;
    let before = client.my_time_entries(date, date).await?.len();
    store.reset_failed(id)?;
    let retried = exec::run_batch(&store, &client, id, &url, me.id, &mut |_| {}).await?;
    assert_eq!(retried.ops[0].status, OpStatus::Done);
    assert_eq!(retried.ops[0].result_id, Some(saved.id), "must adopt the saved entry");
    assert_eq!(client.my_time_entries(date, date).await?.len(), before, "no duplicate");
    exec::undo_batch(&store, &client, id, &url, &mut |_| {}).await?;
    println!("OK: thử lại sau lỗi không chắc chắn nhận entry đã có, không ghi trùng");

    // Redmine rejected the request (422): nothing was created, so a retry
    // creates it normally instead of looking for it.
    let rejected = WritePlan {
        subtasks: vec![],
        items: vec![entry("v1", 1.0, 999_999)],
    };
    let id = store.create_batch("e2e-422", &url, &plan::build_ops(&rejected)?)?;
    let failed = exec::run_batch(&store, &client, id, &url, me.id, &mut |_| {}).await?;
    let err = failed.ops[0].error.clone().unwrap_or_default();
    assert_eq!(failed.ops[0].status, OpStatus::Failed);
    println!("  422: {err}");
    store.reset_failed(id)?;
    assert_eq!(store.batch(id)?.ops[0].status, OpStatus::Pending);
    println!("OK: lỗi Redmine từ chối được thử lại như mục mới");

    // The last check counts what Redmine already holds for the day.
    let held_entry = client
        .create_time_entry(&redmine_client::NewTimeEntry {
            issue_id: parent.id,
            spent_on: date,
            hours: 2.0,
            activity_id: Some(activity.id),
            comments: "e2e-held".into(),
        })
        .await?;
    let logged: Vec<_> = client
        .my_time_entries(date, date)
        .await?
        .into_iter()
        .map(|e| (e.spent_on, e.hours))
        .collect();
    let held: f64 = logged.iter().map(|(_, h)| h).sum();
    let over = WritePlan {
        subtasks: vec![],
        items: vec![entry("o1", 23.0, activity.id)],
    };
    let checked = plan::check_plan(&over, &logged);
    client.delete_time_entry(held_entry.id).await?;
    let msg = checked.unwrap_err().to_string();
    assert!(msg.contains("vượt 24h"), "{msg}");
    println!("OK: chặn 23h khi Redmine đã có {held}h trong ngày: {msg}");
    Ok(())
}
