//! Typed client for the Redmine REST API.
//!
//! Write operations (`create_*`, `delete_*`) are only called by the execution
//! engine after the user has reviewed the plan.

mod types;

use std::time::Duration;

use chrono::NaiveDate;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Response, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use url::Url;

use types::{
    ActivitiesEnvelope, ErrorsEnvelope, IssueEnvelope, IssuesPage, ProjectsPage, TimeEntriesPage,
    TimeEntryEnvelope, TrackersEnvelope, UserEnvelope,
};
pub use types::{
    Activity, IdName, IdRef, Issue, NewIssue, NewTimeEntry, Project, TimeEntry, Tracker, User,
};

const PAGE_SIZE: u32 = 100;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("URL Redmine không hợp lệ: {0}")]
    InvalidUrl(String),
    #[error("API key không hợp lệ")]
    InvalidApiKey,
    #[error("API key bị từ chối (401) — kiểm tra lại key hoặc REST API đã được bật chưa")]
    Unauthorized,
    #[error("Không có quyền truy cập (403)")]
    Forbidden,
    #[error("Không tìm thấy (404): {0}")]
    NotFound(String),
    #[error("Redmine từ chối dữ liệu: {}", .0.join("; "))]
    Validation(Vec<String>),
    #[error("Redmine trả về lỗi {status}: {body}")]
    Http { status: u16, body: String },
    #[error("Lỗi kết nối: {0}")]
    Network(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Default)]
pub struct ClientOptions {
    /// Skip TLS verification for self-signed internal servers.
    pub accept_invalid_certs: bool,
}

#[derive(Debug, Clone)]
pub struct RedmineClient {
    base: Url,
    http: reqwest::Client,
}

impl RedmineClient {
    pub fn new(base_url: &str, api_key: &str, options: ClientOptions) -> Result<Self> {
        let base = normalize_base(base_url)?;

        let mut headers = HeaderMap::new();
        let key = HeaderValue::from_str(api_key.trim()).map_err(|_| Error::InvalidApiKey)?;
        headers.insert("X-Redmine-API-Key", key);

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .tls_danger_accept_invalid_certs(options.accept_invalid_certs)
            .build()?;

        Ok(Self { base, http })
    }

    pub async fn current_user(&self) -> Result<User> {
        let env: UserEnvelope = self.get("users/current.json", &[]).await?;
        Ok(env.user)
    }

    /// All time entries of the current user with `spent_on` in `[from, to]`.
    pub async fn my_time_entries(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<TimeEntry>> {
        self.my_time_entries_in(None, from, to).await
    }

    /// Like [`Self::my_time_entries`], limited to a project (and its sub-projects).
    pub async fn my_time_entries_in(
        &self,
        project_id: Option<u32>,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<TimeEntry>> {
        let from = from.to_string();
        let to = to.to_string();
        let project = project_id.map(|p| p.to_string());
        let mut entries = Vec::new();
        let mut offset = 0u32;
        loop {
            let offset_s = offset.to_string();
            let limit_s = PAGE_SIZE.to_string();
            let mut query = vec![
                ("user_id", "me"),
                ("from", from.as_str()),
                ("to", to.as_str()),
                ("limit", limit_s.as_str()),
                ("offset", offset_s.as_str()),
            ];
            if let Some(p) = &project {
                query.push(("project_id", p.as_str()));
            }
            let page: TimeEntriesPage = self.get("time_entries.json", &query).await?;
            let fetched = page.time_entries.len() as u32;
            entries.extend(page.time_entries);
            offset += fetched;
            if fetched == 0 || offset >= page.total_count {
                break;
            }
        }
        Ok(entries)
    }

    pub async fn time_entry_activities(&self) -> Result<Vec<Activity>> {
        let env: ActivitiesEnvelope = self
            .get("enumerations/time_entry_activities.json", &[])
            .await?;
        Ok(env.time_entry_activities)
    }

    pub async fn issue(&self, id: u32) -> Result<Issue> {
        let env: IssueEnvelope<Issue> = self.get(&format!("issues/{id}.json"), &[]).await?;
        Ok(env.issue)
    }

    /// Issues by id, any status, in pages of 100. Ids the user cannot see are
    /// simply missing from the result.
    pub async fn issues_by_ids(&self, ids: &[u32]) -> Result<Vec<Issue>> {
        let mut issues = Vec::new();
        for chunk in ids.chunks(100) {
            let list = chunk
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let env: IssuesPage = self
                .get(
                    "issues.json",
                    &[("issue_id", &list), ("status_id", "*"), ("limit", "100")],
                )
                .await?;
            issues.extend(env.issues);
        }
        Ok(issues)
    }

    /// Direct children of an issue, any status (first 100).
    pub async fn children(&self, parent_id: u32) -> Result<Vec<Issue>> {
        let parent = parent_id.to_string();
        let env: IssuesPage = self
            .get(
                "issues.json",
                &[("parent_id", &parent), ("status_id", "*"), ("limit", "100")],
            )
            .await?;
        Ok(env.issues)
    }

    /// The current user's direct sub-tasks of an issue, any status.
    pub async fn my_children(&self, parent_id: u32) -> Result<Vec<Issue>> {
        let parent = parent_id.to_string();
        let env: IssuesPage = self
            .get(
                "issues.json",
                &[
                    ("parent_id", &parent),
                    ("assigned_to_id", "me"),
                    ("status_id", "*"),
                    ("sort", "updated_on:desc"),
                    ("limit", "100"),
                ],
            )
            .await?;
        Ok(env.issues)
    }

    /// Open issues whose subject contains `text`, recently updated first.
    pub async fn search_issues(&self, text: &str, limit: u32) -> Result<Vec<Issue>> {
        let subject = format!("~{}", text.trim());
        let limit = limit.to_string();
        let env: IssuesPage = self
            .get(
                "issues.json",
                &[
                    ("subject", &subject),
                    ("status_id", "open"),
                    ("sort", "updated_on:desc"),
                    ("limit", &limit),
                ],
            )
            .await?;
        Ok(env.issues)
    }

    /// Issues with no parent in exactly this project (not its sub-projects),
    /// any status, newest first (first 100).
    pub async fn top_level_issues(&self, project_id: u32) -> Result<Vec<Issue>> {
        let project = project_id.to_string();
        let env: IssuesPage = self
            .get(
                "issues.json",
                &[
                    ("project_id", &project),
                    ("subproject_id", "!*"),
                    ("parent_id", "!*"),
                    ("status_id", "*"),
                    ("sort", "created_on:desc"),
                    ("limit", "100"),
                ],
            )
            .await?;
        Ok(env.issues)
    }

    /// The current user's latest issues in a project, any status.
    pub async fn my_recent_issues(&self, project_id: u32) -> Result<Vec<Issue>> {
        let project = project_id.to_string();
        let env: IssuesPage = self
            .get(
                "issues.json",
                &[
                    ("project_id", &project),
                    ("author_id", "me"),
                    ("status_id", "*"),
                    ("sort", "created_on:desc"),
                    ("limit", "10"),
                ],
            )
            .await?;
        Ok(env.issues)
    }

    /// Every project the user can see.
    pub async fn projects(&self) -> Result<Vec<Project>> {
        let mut projects = Vec::new();
        loop {
            let offset = projects.len().to_string();
            let limit = PAGE_SIZE.to_string();
            let page: ProjectsPage = self
                .get("projects.json", &[("limit", &limit), ("offset", &offset)])
                .await?;
            let fetched = page.projects.len();
            projects.extend(page.projects);
            if fetched == 0 || projects.len() as u32 >= page.total_count {
                break;
            }
        }
        Ok(projects)
    }

    pub async fn trackers(&self) -> Result<Vec<Tracker>> {
        let env: TrackersEnvelope = self.get("trackers.json", &[]).await?;
        Ok(env.trackers)
    }

    pub async fn create_issue(&self, issue: &NewIssue) -> Result<Issue> {
        let env: IssueEnvelope<Issue> = self
            .send(
                reqwest::Method::POST,
                "issues.json",
                &IssueEnvelope { issue },
            )
            .await?
            .json()
            .await?;
        Ok(env.issue)
    }

    pub async fn create_time_entry(&self, entry: &NewTimeEntry) -> Result<TimeEntry> {
        let env: TimeEntryEnvelope<TimeEntry> = self
            .send(
                reqwest::Method::POST,
                "time_entries.json",
                &TimeEntryEnvelope { time_entry: entry },
            )
            .await?
            .json()
            .await?;
        Ok(env.time_entry)
    }

    /// Deleting an entry that is already gone counts as success.
    pub async fn delete_time_entry(&self, id: u32) -> Result<()> {
        let url = self.url(&format!("time_entries/{id}.json"))?;
        match check(self.http.delete(url).send().await?).await {
            Ok(_) | Err(Error::NotFound(_)) => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn url(&self, path: &str) -> Result<Url> {
        self.base
            .join(path)
            .map_err(|e| Error::InvalidUrl(e.to_string()))
    }

    async fn send<B: Serialize>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &B,
    ) -> Result<Response> {
        let resp = self
            .http
            .request(method, self.url(path)?)
            .json(body)
            .send()
            .await?;
        check(resp).await
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let resp = self.http.get(self.url(path)?).query(query).send().await?;
        Ok(check(resp).await?.json().await?)
    }
}

/// Ensures the base URL ends with `/` so relative joins keep sub-paths
/// (e.g. `https://host/redmine`).
fn normalize_base(base_url: &str) -> Result<Url> {
    let trimmed = base_url.trim();
    let with_slash = if trimmed.ends_with('/') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    };
    let url = Url::parse(&with_slash).map_err(|e| Error::InvalidUrl(e.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::InvalidUrl(format!(
            "scheme không hỗ trợ: {}",
            url.scheme()
        )));
    }
    Ok(url)
}

async fn check(resp: Response) -> Result<Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let url = resp.url().path().to_string();
    let body = resp.text().await.unwrap_or_default();
    Err(match status {
        StatusCode::UNAUTHORIZED => Error::Unauthorized,
        StatusCode::FORBIDDEN => Error::Forbidden,
        StatusCode::NOT_FOUND => Error::NotFound(url),
        StatusCode::UNPROCESSABLE_ENTITY => match serde_json::from_str::<ErrorsEnvelope>(&body) {
            Ok(e) => Error::Validation(e.errors),
            Err(_) => Error::Http {
                status: status.as_u16(),
                body,
            },
        },
        _ => Error::Http {
            status: status.as_u16(),
            body: body.chars().take(500).collect(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(server: &MockServer, sub: &str) -> RedmineClient {
        RedmineClient::new(
            &format!("{}{sub}", server.uri()),
            "k123",
            ClientOptions::default(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn issues_by_ids_asks_for_every_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/issues.json"))
            .and(query_param("issue_id", "12,15"))
            .and(query_param("status_id", "*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issues": [{
                    "id": 12, "subject": "Màn hình công nợ",
                    "project": {"id": 1, "name": "ABC"},
                    "tracker": {"id": 2, "name": "Task"},
                    "status": {"id": 5, "name": "Closed"},
                    "author": {"id": 7, "name": "Dev One"}
                }]
            })))
            .mount(&server)
            .await;

        let issues = client(&server, "").issues_by_ids(&[12, 15]).await.unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].subject, "Màn hình công nợ");
    }

    #[test]
    fn base_url_keeps_subpath() {
        let u = normalize_base("https://example.com/redmine").unwrap();
        assert_eq!(
            u.join("users/current.json").unwrap().as_str(),
            "https://example.com/redmine/users/current.json"
        );
        assert!(normalize_base("ftp://x").is_err());
    }

    #[tokio::test]
    async fn current_user_sends_api_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/redmine/users/current.json"))
            .and(header("X-Redmine-API-Key", "k123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "user": {"id": 7, "login": "dev1", "firstname": "Dev", "lastname": "One"}
            })))
            .mount(&server)
            .await;

        let user = client(&server, "/redmine").current_user().await.unwrap();
        assert_eq!(user.id, 7);
        assert_eq!(user.display_name(), "Dev One");
    }

    #[tokio::test]
    async fn unauthorized_maps_to_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let err = client(&server, "").current_user().await.unwrap_err();
        assert!(matches!(err, Error::Unauthorized));
    }

    #[tokio::test]
    async fn create_time_entry_wraps_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/time_entries.json"))
            .and(wiremock::matchers::body_json(json!({"time_entry": {
                "issue_id": 42, "spent_on": "2026-10-28", "hours": 2.5,
                "activity_id": 9, "comments": "Sửa feedback"
            }})))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(json!({"time_entry": {
                    "id": 555, "project": {"id": 1, "name": "P"}, "issue": {"id": 42},
                    "user": {"id": 7, "name": "Dev"}, "activity": {"id": 9, "name": "Dev"},
                    "hours": 2.5, "comments": "Sửa feedback", "spent_on": "2026-10-28"
                }})),
            )
            .mount(&server)
            .await;

        let created = client(&server, "")
            .create_time_entry(&NewTimeEntry {
                issue_id: 42,
                spent_on: NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(),
                hours: 2.5,
                activity_id: Some(9),
                comments: "Sửa feedback".into(),
            })
            .await
            .unwrap();
        assert_eq!(created.id, 555);
    }

    #[tokio::test]
    async fn validation_errors_are_reported() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(422)
                    .set_body_json(json!({"errors": ["Tracker is not included in the list"]})),
            )
            .mount(&server)
            .await;
        let err = client(&server, "")
            .create_issue(&NewIssue {
                project_id: 1,
                tracker_id: 99,
                subject: "x".into(),
                parent_issue_id: Some(5),
                assigned_to_id: None,
                description: None,
                start_date: None,
                due_date: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Validation(ref v) if v[0].contains("Tracker")));
    }

    #[tokio::test]
    async fn delete_missing_entry_is_ok() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/time_entries/9.json"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        client(&server, "").delete_time_entry(9).await.unwrap();
    }

    #[tokio::test]
    async fn time_entries_paginates() {
        let server = MockServer::start().await;
        let entry = |id: u32| {
            json!({
                "id": id, "project": {"id": 1, "name": "P"}, "issue": {"id": 10},
                "user": {"id": 7, "name": "Dev One"}, "activity": {"id": 9, "name": "Dev"},
                "hours": 4.0, "comments": "x", "spent_on": "2026-10-26"
            })
        };
        let first: Vec<_> = (0..100).map(entry).collect();
        Mock::given(path("/time_entries.json"))
            .and(query_param("offset", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"time_entries": first, "total_count": 101, "offset": 0, "limit": 100}),
            ))
            .mount(&server)
            .await;
        Mock::given(path("/time_entries.json"))
            .and(query_param("offset", "100"))
            .and(query_param("user_id", "me"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"time_entries": [entry(100)], "total_count": 101, "offset": 100, "limit": 100}),
            ))
            .mount(&server)
            .await;

        let d = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let entries = client(&server, "")
            .my_time_entries(d, d + chrono::Days::new(30))
            .await
            .unwrap();
        assert_eq!(entries.len(), 101);
    }

    #[tokio::test]
    async fn search_and_my_children_send_filters() {
        let server = MockServer::start().await;
        let issue = |id: u32, subject: &str| {
            json!({
                "id": id, "project": {"id": 1, "name": "P"}, "tracker": {"id": 2, "name": "Feature"},
                "status": {"id": 1, "name": "New"}, "subject": subject, "author": {"id": 5, "name": "BA"}
            })
        };
        Mock::given(path("/issues.json"))
            .and(query_param("subject", "~công nợ"))
            .and(query_param("status_id", "open"))
            .and(query_param("limit", "20"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"issues": [issue(4, "Quản lý công nợ")], "total_count": 1}),
                ),
            )
            .mount(&server)
            .await;
        Mock::given(path("/issues.json"))
            .and(query_param("parent_id", "4"))
            .and(query_param("assigned_to_id", "me"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"issues": [issue(40, "[BE] API công nợ")], "total_count": 1}),
            ))
            .mount(&server)
            .await;

        let c = client(&server, "");
        assert_eq!(c.search_issues(" công nợ ", 20).await.unwrap()[0].id, 4);
        assert_eq!(c.my_children(4).await.unwrap()[0].id, 40);
    }
}
