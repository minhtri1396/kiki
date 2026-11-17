use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use kiki_util::Ctx;
use serde_json::Value;

use super::epic::jira_message;
use super::epic_fields::pick_epic_type;
use super::*;
use crate::atlassian::{Atlassian, Service};
use crate::http::Method;
use crate::testutil::{ok, sample_atlassian, serve, FakeTransport};

#[test]
fn boards_reads_the_agile_list() {
    let fake = FakeTransport::new(|req| {
        assert!(
            req.url.contains("/rest/agile/1.0/board") && req.url.contains("name=KIKI"),
            "url = {}",
            req.url
        );
        ok(200, r#"{"values":[{"id":12,"name":"KIKI"},{"id":"","name":"skip"}]}"#)
    });
    let got = fake
        .api()
        .boards(&Ctx::background(), &sample_atlassian(), "KIKI")
        .unwrap();
    assert!(
        got.len() == 1 && got[0].id == "12" && got[0].name == "KIKI",
        "boards = {got:?}"
    );
}

#[test]
fn spaces_filters_by_name() {
    let fake = FakeTransport::new(|req| {
        if req.url.contains("cursor=next") {
            return ok(200, r#"{"results":[{"id":"2","key":"OPS","name":"Operations"}]}"#);
        }
        ok(
            200,
            r#"{"results":[{"id":"1","key":"ENG","name":"Engineering"}],"_links":{"next":"/wiki/api/v2/spaces?cursor=next"}}"#,
        )
    });
    let got = fake
        .api()
        .spaces(&Ctx::background(), &sample_atlassian(), "ops")
        .unwrap();
    assert!(got.len() == 1 && got[0].key == "OPS", "spaces = {got:?}");
}

#[test]
fn pages_search_uses_the_space_key() {
    let fake = FakeTransport::new(|req| {
        assert!(
            req.url.contains("/wiki/rest/api/content/search")
                && req.url.contains("ENG")
                && req.url.contains("Requirements"),
            "url = {}",
            req.url
        );
        ok(
            200,
            r#"{"results":[{"title":"Requirements","content":{"id":"99","title":"Requirements"}}]}"#,
        )
    });
    let got = fake
        .api()
        .pages(&Ctx::background(), &sample_atlassian(), "111", "ENG", "Requirements")
        .unwrap();
    assert!(
        got.len() == 1 && got[0].id == "99" && got[0].name == "Requirements",
        "pages = {got:?}"
    );
}

#[test]
fn pages_without_a_space_is_an_error() {
    let fake = FakeTransport::new(|_| panic!("no request expected"));
    let err = fake
        .api()
        .pages(&Ctx::background(), &sample_atlassian(), " ", "", "q")
        .unwrap_err();
    assert_eq!(err.to_string(), "choose a Confluence space first");
}

#[test]
fn boards_reports_a_rejected_token() {
    let fake = FakeTransport::new(|_| ok(401, r#"{"message":"client must be authenticated"}"#));
    let err = fake
        .api()
        .boards(&Ctx::background(), &sample_atlassian(), "")
        .unwrap_err();
    assert!(err.to_string().contains("rejected"), "err = {err}");
}

#[test]
fn transport_failures_name_the_call() {
    let fake = FakeTransport::new(|_| Err(anyhow!("connection refused\nretry")));
    let err = fake
        .api()
        .boards(&Ctx::background(), &sample_atlassian(), "")
        .unwrap_err();
    assert_eq!(err.to_string(), "Jira boards: connection refused retry");
}

#[test]
fn boards_needs_the_saved_token() {
    let cfg = Atlassian {
        enabled: true,
        jira: Service {
            enabled: true,
            url: "https://acme.atlassian.net".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let err = AtlassianApi::default()
        .boards(&Ctx::background(), &cfg, "")
        .unwrap_err();
    assert!(err.to_string().contains("API token"), "err = {err}");
}

#[test]
fn boards_over_real_http_filters_and_reports_errors() {
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let fake = serve(2, move |_, _| {
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            (200, r#"{"values":[{"id":7,"name":" Board "}]}"#.into())
        } else {
            (500, "boom\nboom".into())
        }
    });
    let api = AtlassianApi::default();
    let mut cfg = sample_atlassian();
    cfg.jira.url = format!("{}/", fake.base);
    let got = api.boards(&Ctx::background(), &cfg, "Bo ard").unwrap();
    assert_eq!(
        got,
        vec![Hit {
            id: "7".into(),
            name: "Board".into(),
            key: String::new()
        }]
    );
    let err = api.boards(&Ctx::background(), &cfg, "").unwrap_err();
    assert_eq!(err.to_string(), "Jira boards: boom boom");
    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen[0].1, "/rest/agile/1.0/board?maxResults=50&name=Bo+ard");
    assert_eq!(seen[1].1, "/rest/agile/1.0/board?maxResults=50");
}

#[test]
fn create_epic_uses_the_settings_board() {
    let mut cfg = sample_atlassian();
    cfg.jira.board_id = "12".into();
    cfg.jira.board_name = "KIKI".into();
    let fake = FakeTransport::new(|req| {
        if req.method == Method::Post {
            assert!(req.url.ends_with("/rest/api/3/issue"), "url = {}", req.url);
            return ok(201, r#"{"id":"10008","key":"kiki-8"}"#);
        }
        let url = req.url;
        if url.contains("/rest/agile/1.0/board/12") && !url.contains("/project") {
            ok(200, r#"{"name":"KIKI","location":{"projectKey":"KIKI"}}"#)
        } else if url.ends_with("/issuetypes") {
            ok(
                200,
                r#"{"issueTypes":[{"id":"10001","name":"Epic","hierarchyLevel":1}]}"#,
            )
        } else if url.contains("/issuetypes/10001") {
            ok(
                200,
                r#"{"fields":[{"fieldId":"summary","name":"Summary","required":true},{"fieldId":"customfield_10011","name":"Epic Name","required":true,"schema":{"custom":"com.pyxis.greenhopper.jira:gh-epic-label"}},{"fieldId":"description","name":"Description","required":false,"schema":{"system":"description"}}]}"#,
            )
        } else {
            panic!("url = {url}");
        }
    });
    let got = fake
        .api()
        .create_epic(&Ctx::background(), &cfg, " Ship billing ", "Add the invoice page.")
        .unwrap();
    assert!(
        got.key == "KIKI-8"
            && got.id == "10008"
            && got.board_id == "12"
            && got.board_name == "KIKI"
            && got.url == "https://acme.atlassian.net/browse/KIKI-8",
        "epic = {got:?}"
    );
    let seen = fake.seen.lock().unwrap();
    let posted = seen
        .iter()
        .find(|s| s.method == Method::Post)
        .unwrap()
        .payload
        .clone()
        .unwrap();
    let body: Value = serde_json::from_slice(&posted).unwrap();
    let fields = &body["fields"];
    assert!(
        fields["summary"] == "Ship billing" && fields["customfield_10011"] == "Ship billing",
        "fields = {fields:?}"
    );
    assert!(
        fields["project"]["key"] == "KIKI" && fields["issuetype"]["id"] == "10001",
        "fields = {fields:?}"
    );
    assert_eq!(
        fields["description"]["type"], "doc",
        "description = {:?}",
        fields["description"]
    );
}

#[test]
fn create_epic_needs_a_board() {
    let err = AtlassianApi::default()
        .create_epic(&Ctx::background(), &sample_atlassian(), "Ship billing", "")
        .unwrap_err();
    assert!(err.to_string().contains("Jira board"), "err = {err}");
}

#[test]
fn create_epic_reports_jira() {
    let mut cfg = sample_atlassian();
    cfg.jira.board_id = "12".into();
    let fake = FakeTransport::new(|req| {
        if req.method == Method::Post {
            return ok(
                400,
                r#"{"errorMessages":["Epic Name is required"],"errors":{"customfield_10011":"Epic Name is required"}}"#,
            );
        }
        if req.url.contains("/issuetype") {
            return ok(404, r#"{"message":"missing"}"#);
        }
        ok(200, r#"{"name":"KIKI","location":{"projectKey":"KIKI"}}"#)
    });
    let err = fake
        .api()
        .create_epic(&Ctx::background(), &cfg, "Ship billing", "")
        .unwrap_err();
    assert!(err.to_string().contains("Epic Name is required"), "err = {err}");
}

#[test]
fn delete_issue_cases() {
    let cfg = sample_atlassian();
    let status = Arc::new(Mutex::new((204u16, String::new())));
    let answer = status.clone();
    let fake = FakeTransport::new(move |req| {
        assert_eq!(req.method, Method::Delete);
        let (code, body) = answer.lock().unwrap().clone();
        ok(code, &body)
    });
    let api = fake.api();
    api.delete_issue(&Ctx::background(), &cfg, "kiki-12").unwrap();
    assert_eq!(
        fake.seen.lock().unwrap()[0].url,
        "https://acme.atlassian.net/rest/api/3/issue/KIKI-12"
    );

    *status.lock().unwrap() = (404, r#"{"errorMessages":["Issue does not exist"]}"#.into());
    api.delete_issue(&Ctx::background(), &cfg, "KIKI-12").unwrap();

    *status.lock().unwrap() = (
        400,
        r#"{"errorMessages":["The issue can not be deleted because it has sub-tasks."]}"#.into(),
    );
    let err = api.delete_issue(&Ctx::background(), &cfg, "KIKI-12").unwrap_err();
    assert!(err.to_string().contains("sub-tasks"), "err = {err}");
    assert!(
        api.delete_issue(&Ctx::background(), &cfg, "not a key").is_err(),
        "expected a bad key to fail"
    );
}

#[test]
fn delete_issue_over_real_http() {
    let fake = serve(1, |_, _| (204, String::new()));
    let mut cfg = sample_atlassian();
    cfg.jira.url = fake.base.clone();
    AtlassianApi::default()
        .delete_issue(&Ctx::background(), &cfg, "KIKI-1")
        .unwrap();
    let seen = fake.seen.lock().unwrap();
    assert_eq!(
        (seen[0].0.as_str(), seen[0].1.as_str()),
        ("DELETE", "/rest/api/3/issue/KIKI-1")
    );
}

#[test]
fn jira_message_joins_parts() {
    assert_eq!(jira_message(br#"{"errorMessages":["a","b"],"message":"c"}"#), "a b c");
    assert_eq!(jira_message(b"plain\ntext"), "plain text");
    assert_eq!(jira_message(b"{}"), "{}");
}

#[test]
fn issue_types_array_body_is_rejected() {
    let err = pick_epic_type(br#"[{"id":"1","name":"Epic"}]"#).unwrap_err();
    assert!(err.to_string().starts_with("Jira epic: "), "err = {err}");
}
