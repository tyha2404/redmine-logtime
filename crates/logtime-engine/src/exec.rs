//! Runs a batch against Redmine. Every step is recorded before and after the
//! request so a retry never creates the same sub-task or entry twice.

use redmine_client::{NewIssue, NewTimeEntry, RedmineClient};

use crate::model::{Batch, Op, OpPayload, OpStatus, PlanItem, SubtaskDraft, Target};
use crate::store::Store;
use crate::{Error, Result};

const EPS: f64 = 1e-6;

pub type Progress<'a> = &'a mut (dyn FnMut(&Op) + Send);

/// Runs every pending or interrupted op of the batch. Failed ops stay failed
/// until [`Store::reset_failed`] queues them for another run.
pub async fn run_batch(
    store: &Store,
    client: &RedmineClient,
    batch_id: i64,
    current_url: &str,
    assignee_id: u32,
    on_progress: Progress<'_>,
) -> Result<Batch> {
    let batch = store.batch(batch_id)?;
    ensure_same_server(&batch, current_url)?;

    for op in batch.ops {
        if !matches!(op.status, OpStatus::Pending | OpStatus::Running) {
            continue;
        }
        let interrupted = op.status == OpStatus::Running;
        store.set_op(op.id, OpStatus::Running, None, None)?;

        let outcome = match &op.payload {
            OpPayload::CreateSubtask(sub) => {
                create_subtask(store, client, batch_id, sub, assignee_id, interrupted).await
            }
            OpPayload::CreateTimeEntry(item) => {
                create_entry(store, client, item, interrupted).await
            }
        };

        let (status, result_id, error) = match outcome {
            Ok(id) => (OpStatus::Done, Some(id), None),
            Err(e) => (OpStatus::Failed, None, Some(e.to_string())),
        };
        store.set_op(op.id, status, result_id, error.as_deref())?;
        on_progress(&Op {
            status,
            result_id: result_id.or(op.result_id),
            error,
            ..op
        });
    }
    store.batch(batch_id)
}

/// Deletes the time entries the batch created. Sub-tasks are kept: most
/// Redmine roles cannot delete issues.
pub async fn undo_batch(
    store: &Store,
    client: &RedmineClient,
    batch_id: i64,
    current_url: &str,
    on_progress: Progress<'_>,
) -> Result<Batch> {
    let batch = store.batch(batch_id)?;
    ensure_same_server(&batch, current_url)?;

    for op in batch.ops.into_iter().rev() {
        let (OpPayload::CreateTimeEntry(_), OpStatus::Done, Some(entry_id)) =
            (&op.payload, op.status, op.result_id)
        else {
            continue;
        };
        let (status, error) = match client.delete_time_entry(entry_id).await {
            Ok(()) => (OpStatus::Undone, None),
            Err(e) => (OpStatus::Done, Some(format!("Không xoá được: {e}"))),
        };
        store.set_op(op.id, status, None, error.as_deref())?;
        on_progress(&Op {
            status,
            error,
            ..op
        });
    }
    store.batch(batch_id)
}

fn ensure_same_server(batch: &Batch, current_url: &str) -> Result<()> {
    let norm = |u: &str| u.trim().trim_end_matches('/').to_lowercase();
    if norm(&batch.redmine_url) == norm(current_url) {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "Lô này đã ghi lên {}, khác với Redmine đang cấu hình.",
            batch.redmine_url
        )))
    }
}

async fn create_subtask(
    store: &Store,
    client: &RedmineClient,
    batch_id: i64,
    sub: &SubtaskDraft,
    assignee_id: u32,
    interrupted: bool,
) -> Result<u32> {
    if let Some(id) = store.subtask_issue(&sub.key)? {
        return Ok(id);
    }
    // The previous attempt may have created it before the app stopped.
    if interrupted
        && let Some(found) = client
            .children(sub.parent_id)
            .await?
            .into_iter()
            .find(|c| c.subject == sub.subject)
    {
        store.save_subtask(&sub.key, found.id, batch_id)?;
        return Ok(found.id);
    }
    let issue = client
        .create_issue(&NewIssue {
            project_id: sub.project_id,
            tracker_id: sub.tracker_id,
            subject: sub.subject.clone(),
            parent_issue_id: Some(sub.parent_id),
            assigned_to_id: Some(assignee_id),
            description: None,
            start_date: sub.start_date,
            due_date: sub.start_date,
        })
        .await?;
    store.save_subtask(&sub.key, issue.id, batch_id)?;
    Ok(issue.id)
}

async fn create_entry(
    store: &Store,
    client: &RedmineClient,
    item: &PlanItem,
    interrupted: bool,
) -> Result<u32> {
    let issue_id = match &item.target {
        Target::Issue { issue_id, .. } => *issue_id,
        Target::NewSubtask { key } => store.subtask_issue(key)?.ok_or_else(|| {
            Error::Invalid("Task con chưa được tạo nên chưa ghi được logtime này.".into())
        })?,
    };
    if interrupted {
        let same_day = client.my_time_entries(item.date, item.date).await?;
        if let Some(found) = same_day.iter().find(|e| {
            e.issue.as_ref().is_some_and(|i| i.id == issue_id)
                && (e.hours - item.hours).abs() < EPS
                && e.comments.as_deref().unwrap_or("") == item.comments
        }) {
            return Ok(found.id);
        }
    }
    let entry = client
        .create_time_entry(&NewTimeEntry {
            issue_id,
            spent_on: item.date,
            hours: item.hours,
            activity_id: item.activity_id,
            comments: item.comments.clone(),
        })
        .await?;
    Ok(entry.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ItemKind, WritePlan};
    use crate::plan::build_ops;
    use chrono::NaiveDate;
    use redmine_client::ClientOptions;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn plan() -> WritePlan {
        WritePlan {
            subtasks: vec![SubtaskDraft {
                key: "k1".into(),
                parent_id: 1,
                parent_subject: "Công nợ".into(),
                project_id: 3,
                project_name: "ABC".into(),
                tracker_id: 4,
                subject: "Sửa feedback công nợ".into(),
                start_date: None,
            }],
            items: vec![
                PlanItem {
                    id: "a".into(),
                    date: NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(),
                    target: Target::NewSubtask { key: "k1".into() },
                    hours: 5.0,
                    activity_id: Some(9),
                    comments: "Sửa feedback".into(),
                    kind: ItemKind::Work,
                },
                PlanItem {
                    id: "b".into(),
                    date: NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(),
                    target: Target::Issue {
                        issue_id: 50,
                        label: "#50".into(),
                    },
                    hours: 3.0,
                    activity_id: Some(9),
                    comments: "Review".into(),
                    kind: ItemKind::Work,
                },
            ],
        }
    }

    fn entry_json(id: u32, issue: u32, hours: f64) -> serde_json::Value {
        json!({"time_entry": {
            "id": id, "project": {"id": 3, "name": "ABC"}, "issue": {"id": issue},
            "user": {"id": 7, "name": "Me"}, "activity": {"id": 9, "name": "Dev"},
            "hours": hours, "comments": "", "spent_on": "2026-10-28"
        }})
    }

    async fn setup(server: &MockServer) -> (Store, RedmineClient, i64) {
        let store = Store::open_in_memory().unwrap();
        let client = RedmineClient::new(&server.uri(), "k", ClientOptions::default()).unwrap();
        let ops = build_ops(&plan()).unwrap();
        let id = store.create_batch("test", &server.uri(), &ops).unwrap();
        (store, client, id)
    }

    #[tokio::test]
    async fn failures_stay_until_reset_then_retry_only_them() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/issues.json"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"issue": {
                "id": 77, "project": {"id": 3, "name": "ABC"}, "tracker": {"id": 4, "name": "Task"},
                "status": {"id": 1, "name": "New"}, "subject": "Sửa feedback công nợ",
                "author": {"id": 7, "name": "Me"}
            }})))
            .expect(1)
            .mount(&server)
            .await;
        // Entry on the new sub-task succeeds; the one on #50 fails the first time.
        Mock::given(method("POST"))
            .and(path("/time_entries.json"))
            .and(wiremock::matchers::body_partial_json(
                json!({"time_entry": {"issue_id": 77}}),
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(entry_json(501, 77, 5.0)))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/time_entries.json"))
            .and(wiremock::matchers::body_partial_json(
                json!({"time_entry": {"issue_id": 50}}),
            ))
            .respond_with(
                ResponseTemplate::new(422).set_body_json(json!({"errors": ["Issue is invalid"]})),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let (store, client, id) = setup(&server).await;
        let mut seen = Vec::new();
        let batch = run_batch(&store, &client, id, &server.uri(), 7, &mut |op: &Op| {
            seen.push(op.status)
        })
        .await
        .unwrap();

        assert_eq!(seen, vec![OpStatus::Done, OpStatus::Done, OpStatus::Failed]);
        assert_eq!(batch.ops[0].result_id, Some(77));
        assert!(
            batch.ops[2]
                .error
                .as_deref()
                .unwrap()
                .contains("Issue is invalid")
        );

        // Running again sends nothing: done ops are skipped, failures are final.
        let again = run_batch(&store, &client, id, &server.uri(), 7, &mut |_: &Op| {})
            .await
            .unwrap();
        assert_eq!(again, batch);

        // A retry resets only the failure; the sub-task and first entry are not sent again.
        Mock::given(method("POST"))
            .and(path("/time_entries.json"))
            .and(wiremock::matchers::body_partial_json(
                json!({"time_entry": {"issue_id": 50}}),
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(entry_json(502, 50, 3.0)))
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(store.reset_failed(id).unwrap(), 1);
        let retried = run_batch(&store, &client, id, &server.uri(), 7, &mut |_: &Op| {})
            .await
            .unwrap();
        assert!(retried.ops.iter().all(|o| o.status == OpStatus::Done));
        assert_eq!(retried.ops[2].result_id, Some(502));
        // `.expect(1)` on the issue/first entry mocks verifies no duplicates on drop.
    }

    #[tokio::test]
    async fn interrupted_entry_adopts_existing_instead_of_duplicating() {
        let server = MockServer::start().await;
        let (store, client, id) = setup(&server).await;
        let batch = store.batch(id).unwrap();
        store.save_subtask("k1", 77, id).unwrap();
        store
            .set_op(batch.ops[0].id, OpStatus::Done, Some(77), None)
            .unwrap();
        store
            .set_op(batch.ops[1].id, OpStatus::Running, None, None)
            .unwrap();
        store
            .set_op(batch.ops[2].id, OpStatus::Done, Some(502), None)
            .unwrap();

        // The entry for the interrupted op already exists on Redmine.
        Mock::given(method("GET"))
            .and(path("/time_entries.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "time_entries": [{
                    "id": 501, "project": {"id": 3, "name": "ABC"}, "issue": {"id": 77},
                    "user": {"id": 7, "name": "Me"}, "activity": {"id": 9, "name": "Dev"},
                    "hours": 5.0, "comments": "Sửa feedback", "spent_on": "2026-10-28"
                }],
                "total_count": 1
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;

        let batch = run_batch(&store, &client, id, &server.uri(), 7, &mut |_: &Op| {})
            .await
            .unwrap();
        assert_eq!(batch.ops[1].status, OpStatus::Done);
        assert_eq!(batch.ops[1].result_id, Some(501));
    }

    #[tokio::test]
    async fn undo_deletes_entries_only() {
        let server = MockServer::start().await;
        let (store, client, id) = setup(&server).await;
        let batch = store.batch(id).unwrap();
        store
            .set_op(batch.ops[0].id, OpStatus::Done, Some(77), None)
            .unwrap();
        store
            .set_op(batch.ops[1].id, OpStatus::Done, Some(501), None)
            .unwrap();
        store
            .set_op(batch.ops[2].id, OpStatus::Done, Some(502), None)
            .unwrap();

        Mock::given(method("DELETE"))
            .and(path("/time_entries/501.json"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/time_entries/502.json"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let batch = undo_batch(&store, &client, id, &server.uri(), &mut |_: &Op| {})
            .await
            .unwrap();
        let statuses: Vec<_> = batch.ops.iter().map(|o| o.status).collect();
        assert_eq!(
            statuses,
            vec![OpStatus::Done, OpStatus::Undone, OpStatus::Undone]
        );
    }

    #[tokio::test]
    async fn refuses_other_server() {
        let server = MockServer::start().await;
        let (store, client, id) = setup(&server).await;
        let err = run_batch(
            &store,
            &client,
            id,
            "https://other.example",
            7,
            &mut |_: &Op| {},
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("khác với Redmine"));
    }
}
