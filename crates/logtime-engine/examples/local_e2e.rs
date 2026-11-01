//! End-to-end check of the write engine against the local Docker Redmine
//! (docker/). Refuses to run against anything but localhost.
//!
//!   REDMINE_API_KEY=<dev1 key> cargo run -p logtime-engine --example local_e2e

use chrono::NaiveDate;
use logtime_engine::model::{Draft, ItemKind, OpStatus, PlanItem, SubtaskDraft, Target};
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

    let draft = Draft {
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
            last_error: None,
        }],
    };

    let store = Store::open_in_memory()?;
    let ops = plan::build_ops(&draft, &["i1".into()])?;
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
    Ok(())
}
