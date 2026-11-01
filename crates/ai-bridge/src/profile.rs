//! "Project profile": what the model learned about how this Redmine is
//! organised — mainly how to recognise BA-created parent tasks.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ParentRule {
    pub tracker_ids: Vec<u32>,
    pub tracker_names: Vec<String>,
    pub author_ids: Vec<u32>,
    pub author_names: Vec<String>,
    pub role_names: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProjectRef {
    pub id: u32,
    pub name: String,
    pub identifier: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Profile {
    pub parent_rule: ParentRule,
    pub subtask_tracker_id: Option<u32>,
    pub leave_issue_id: Option<u32>,
    pub projects: Vec<ProjectRef>,
    pub naming: String,
    pub summary: String,
    /// Written by the user, never overwritten by a new discovery.
    pub user_notes: String,
    pub generated_at: Option<String>,
}

pub fn schema() -> Value {
    let ids = json!({"type": "array", "items": {"type": "integer"}});
    let strings = json!({"type": "array", "items": {"type": "string"}});
    json!({
        "type": "object",
        "required": ["parent_rule", "subtask_tracker_id", "leave_issue_id", "projects", "naming", "summary"],
        "properties": {
            "parent_rule": {
                "type": "object",
                "required": ["tracker_ids", "tracker_names", "author_ids", "author_names", "role_names", "description"],
                "properties": {
                    "tracker_ids": ids, "tracker_names": strings,
                    "author_ids": ids, "author_names": strings, "role_names": strings,
                    "description": {"type": "string"}
                }
            },
            "subtask_tracker_id": {"type": ["integer", "null"]},
            "leave_issue_id": {"type": ["integer", "null"]},
            "projects": {"type": "array", "items": {
                "type": "object",
                "required": ["id", "name", "identifier"],
                "properties": {"id": {"type": "integer"}, "name": {"type": "string"}, "identifier": {"type": "string"}}
            }},
            "naming": {"type": "string"},
            "summary": {"type": "string"}
        }
    })
}

pub fn discovery_prompt() -> String {
    r#"You are analysing a company's Redmine through the `redmine` MCP tools (read-only).

Context: developers must log time on a SUB-TASK they create under a PARENT task that a BA (business analyst) created. Your job is to work out, from real data, how to recognise those BA parent tasks, so a tool can later match a developer's work report to the right parent.

Investigate with `redmine_request` (GET only, keep it under ~40 requests, use limit=100):
1. GET /users/current.json — who the developer is.
2. GET /projects.json and GET /trackers.json.
3. GET /time_entries.json?user_id=me&limit=100 — the issues this developer logs time on. For a sample of those issues GET /issues/{id}.json and follow `parent` upwards: note tracker, author and subject of the parents vs. the sub-tasks.
4. For the 1–5 projects the developer is most active in: GET /projects/{id}/memberships.json (roles: who is BA / Business Analyst / PO / Tester…), and GET /issues.json?project_id={id}&status_id=*&sort=updated_on:desc&limit=100 to see which trackers and authors have children.

Decide:
- parent_rule: trackers and authors/roles that define a BA parent task, plus a short Vietnamese `description` a model can follow (e.g. "Task cha là issue tracker Feature do thành viên role BA tạo, thường chưa có parent").
- subtask_tracker_id: the tracker developers use for their sub-tasks (null if unclear).
- leave_issue_id: always null (the app handles leave itself).
- projects: the projects the developer works in (id, name, identifier).
- naming: Vietnamese note on sub-task naming conventions seen (prefixes like [FE]/[BE], language…).
- summary: 3–8 Vietnamese sentences describing the structure, written as guidance for a later model that must find the right parent for a report line.

Only state what the data supports; say so in the summary when something is uncertain."#
        .to_string()
}

/// Markdown handed to the MCP server as REDMINE_REQUEST_INSTRUCTIONS and
/// embedded in later prompts.
pub fn to_instructions(p: &Profile) -> String {
    let mut s = String::from("# Cấu trúc Redmine của người dùng\n\n");
    if !p.summary.is_empty() {
        s += &format!("{}\n\n", p.summary);
    }
    s += "## Cách nhận biết task cha (do BA tạo)\n\n";
    s += &format!("{}\n", p.parent_rule.description);
    let r = &p.parent_rule;
    if !r.tracker_ids.is_empty() {
        s += &format!(
            "- Tracker task cha: {} (id {:?})\n",
            r.tracker_names.join(", "),
            r.tracker_ids
        );
    }
    if !r.author_ids.is_empty() || !r.role_names.is_empty() {
        s += &format!(
            "- Người tạo: {} (id {:?}); role: {}\n",
            r.author_names.join(", "),
            r.author_ids,
            r.role_names.join(", ")
        );
    }
    if let Some(t) = p.subtask_tracker_id {
        s += &format!("- Tracker cho task con: id {t}\n");
    }
    if !p.projects.is_empty() {
        s += "\n## Dự án\n\n";
        for pr in &p.projects {
            s += &format!(
                "- {} (id {}, identifier `{}`)\n",
                pr.name, pr.id, pr.identifier
            );
        }
    }
    if !p.naming.is_empty() {
        s += &format!("\n## Đặt tên task con\n\n{}\n", p.naming);
    }
    if !p.user_notes.trim().is_empty() {
        s += &format!(
            "\n## Ghi chú của người dùng (ưu tiên cao nhất)\n\n{}\n",
            p.user_notes.trim()
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_include_rule_and_user_notes() {
        let p = Profile {
            parent_rule: ParentRule {
                tracker_ids: vec![2],
                tracker_names: vec!["Feature".into()],
                role_names: vec!["BA".into()],
                description: "Task cha là Feature do BA tạo.".into(),
                ..Default::default()
            },
            leave_issue_id: Some(6),
            user_notes: "Dự án Salex dùng prefix [SX].".into(),
            ..Default::default()
        };
        let md = to_instructions(&p);
        assert!(md.contains("Feature (id [2])"));
        // Leave is handled by the app, not by the profile.
        assert!(!md.contains("#6"));
        assert!(md.contains("prefix [SX]"));
    }

    #[test]
    fn model_output_deserializes_without_user_fields() {
        let raw = json!({
            "parent_rule": {"tracker_ids": [2], "tracker_names": ["Feature"], "author_ids": [5],
                            "author_names": ["Lan"], "role_names": ["Manager"], "description": "x"},
            "subtask_tracker_id": 4, "leave_issue_id": null, "projects": [], "naming": "", "summary": "s"
        });
        let p: Profile = serde_json::from_value(raw).unwrap();
        assert_eq!(p.subtask_tracker_id, Some(4));
        assert!(p.user_notes.is_empty());
    }
}
