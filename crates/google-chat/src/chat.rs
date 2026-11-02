//! The few Google Chat REST calls the app needs, all reads.

use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{Endpoints, Error, Result, error_message, http_client};

const PAGE_SIZE: &str = "1000";
/// A runaway pagination guard: 50 pages of 1000 messages.
const MAX_PAGES: usize = 50;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Space {
    /// `spaces/AAAA…`
    pub name: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Message {
    /// `spaces/…/messages/…`
    pub name: String,
    pub text: String,
    pub create_time: DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpacesPage {
    #[serde(default)]
    spaces: Vec<RawSpace>,
    #[serde(default)]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSpace {
    name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    space_type: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessagesPage {
    #[serde(default)]
    messages: Vec<RawMessage>,
    #[serde(default)]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMessage {
    name: String,
    #[serde(default)]
    sender: Option<Sender>,
    create_time: DateTime<Utc>,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct Sender {
    #[serde(default)]
    name: String,
}

pub struct ChatClient {
    base: String,
    token: String,
    http: reqwest::Client,
}

impl ChatClient {
    pub fn new(access_token: &str, endpoints: &Endpoints) -> Result<Self> {
        Ok(Self {
            base: endpoints.chat.trim_end_matches('/').to_string(),
            token: access_token.to_string(),
            http: http_client()?,
        })
    }

    /// Named spaces and group chats the user is in; direct messages are left
    /// out (they have no name to pick them by).
    pub async fn spaces(&self) -> Result<Vec<Space>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut query = vec![("pageSize", PAGE_SIZE.to_string())];
            if let Some(t) = &token {
                query.push(("pageToken", t.clone()));
            }
            let page: SpacesPage = self.get("/v1/spaces", &query).await?;
            out.extend(
                page.spaces
                    .into_iter()
                    .filter_map(|s| match s.space_type.as_str() {
                        "DIRECT_MESSAGE" => None,
                        _ => Some(Space {
                            display_name: if s.display_name.trim().is_empty() {
                                "Nhóm chat không tên".into()
                            } else {
                                s.display_name
                            },
                            name: s.name,
                        }),
                    }),
            );
            token = page.next_page_token.filter(|t| !t.is_empty());
            if token.is_none() {
                break;
            }
        }
        out.sort_by_key(|s| s.display_name.to_lowercase());
        Ok(out)
    }

    /// Messages that `user_id` (the OIDC `sub`) posted in `space` between
    /// `from` and `to`, oldest first. Thread replies are included.
    pub async fn my_messages(
        &self,
        space: &str,
        user_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<Message>> {
        if !valid_space(space) {
            return Err(Error::Invalid(format!("tên group không hợp lệ: {space}")));
        }
        let me = format!("users/{user_id}");
        let filter = format!(
            "createTime > \"{}\" AND createTime < \"{}\"",
            from.to_rfc3339_opts(SecondsFormat::Secs, true),
            to.to_rfc3339_opts(SecondsFormat::Secs, true),
        );
        let path = format!("/v1/{space}/messages");
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut query = vec![
                ("pageSize", PAGE_SIZE.to_string()),
                ("filter", filter.clone()),
                ("orderBy", "createTime ASC".to_string()),
            ];
            if let Some(t) = &token {
                query.push(("pageToken", t.clone()));
            }
            let page: MessagesPage = self.get(&path, &query).await?;
            out.extend(
                page.messages
                    .into_iter()
                    .filter(|m| m.sender.as_ref().is_some_and(|s| s.name == me))
                    .filter(|m| !m.text.trim().is_empty())
                    .map(|m| Message {
                        name: m.name,
                        text: m.text,
                        create_time: m.create_time,
                    }),
            );
            token = page.next_page_token.filter(|t| !t.is_empty());
            if token.is_none() {
                break;
            }
        }
        Ok(out)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        let res = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .query(query)
            .send()
            .await?;
        let status = res.status();
        let body = res.text().await?;
        match status {
            s if s.is_success() => {
                serde_json::from_str(&body).map_err(|e| Error::Invalid(e.to_string()))
            }
            StatusCode::UNAUTHORIZED => Err(Error::Revoked),
            StatusCode::FORBIDDEN => Err(Error::Forbidden(error_message(&body))),
            StatusCode::NOT_FOUND => Err(Error::NotFound),
            s => Err(Error::Http {
                status: s.as_u16(),
                message: error_message(&body),
            }),
        }
    }
}

/// `spaces/<id>` only, so a stored value cannot reach another API path.
fn valid_space(space: &str) -> bool {
    space.strip_prefix("spaces/").is_some_and(|id| {
        !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn client(server: &MockServer) -> ChatClient {
        let endpoints = Endpoints {
            chat: server.uri(),
            ..Endpoints::default()
        };
        ChatClient::new("at", &endpoints).unwrap()
    }

    fn msg(id: u32, sender: &str, text: &str) -> serde_json::Value {
        json!({
            "name": format!("spaces/S1/messages/{id}"),
            "sender": {"name": sender, "type": "HUMAN"},
            "createTime": format!("2026-10-26T0{id}:00:00.123456Z"),
            "text": text,
        })
    }

    #[tokio::test]
    async fn spaces_skip_direct_messages_and_follow_pages() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/spaces"))
            .and(header("authorization", "Bearer at"))
            .and(query_param_is_missing("pageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "spaces": [
                    {"name": "spaces/B", "displayName": "Team báo cáo", "spaceType": "SPACE"},
                    {"name": "spaces/D", "spaceType": "DIRECT_MESSAGE"},
                ],
                "nextPageToken": "p2",
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/spaces"))
            .and(query_param("pageToken", "p2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "spaces": [{"name": "spaces/A", "spaceType": "GROUP_CHAT"}],
            })))
            .mount(&server)
            .await;
        let spaces = client(&server).await.spaces().await.unwrap();
        let names: Vec<_> = spaces
            .iter()
            .map(|s| (s.name.as_str(), s.display_name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("spaces/A", "Nhóm chat không tên"),
                ("spaces/B", "Team báo cáo")
            ]
        );
    }

    #[tokio::test]
    async fn messages_keep_only_mine_in_range() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/spaces/S1/messages"))
            .and(query_param(
                "filter",
                "createTime > \"2026-10-25T17:00:00Z\" AND createTime < \"2026-10-26T17:00:00Z\"",
            ))
            .and(query_param("orderBy", "createTime ASC"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "messages": [
                    msg(1, "users/42", "Thứ 2 26/10:\n- Sửa feedback công nợ"),
                    msg(2, "users/7", "Báo cáo của người khác"),
                    msg(3, "users/42", "   "),
                    {"name": "spaces/S1/messages/4", "createTime": "2026-10-26T04:00:00Z", "text": "app"},
                ],
            })))
            .mount(&server)
            .await;
        let from = Utc.with_ymd_and_hms(2026, 10, 25, 17, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2026, 10, 26, 17, 0, 0).unwrap();
        let got = client(&server)
            .await
            .my_messages("spaces/S1", "42", from, to)
            .await
            .unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "spaces/S1/messages/1");
        assert!(got[0].text.starts_with("Thứ 2"));
    }

    #[tokio::test]
    async fn errors_map_to_actionable_messages() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/spaces"))
            .respond_with(ResponseTemplate::new(403).set_body_json(json!({
                "error": {"code": 403, "message": "Google Chat API has not been used in project 1"}
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/spaces/GONE/messages"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let c = client(&server).await;
        match c.spaces().await {
            Err(Error::Forbidden(m)) => assert!(m.contains("has not been used")),
            other => panic!("{other:?}"),
        }
        let t = Utc::now();
        assert!(matches!(
            c.my_messages("spaces/GONE", "42", t, t).await,
            Err(Error::NotFound)
        ));
    }

    #[test]
    fn space_names_are_validated() {
        assert!(valid_space("spaces/AAAA_b-9"));
        for bad in [
            "spaces/",
            "spaces/../x",
            "users/me",
            "spaces/a/messages",
            "spaces/a?b",
        ] {
            assert!(!valid_space(bad), "{bad}");
        }
    }
}
