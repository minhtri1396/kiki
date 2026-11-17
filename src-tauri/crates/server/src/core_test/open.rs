//! Opening pages and Jira tickets in the browser.

use super::*;

#[test]
fn page_command_opens_only_index_pages() {
    let cmd = crate::open::page_command("https://obsidian.md/help/cli").unwrap();
    let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    if cfg!(target_os = "macos") {
        assert!(
            !cmd.get_program().is_empty() && args.len() == 1 && args[0] == "https://obsidian.md/help/cli",
            "args = {args:?}"
        );
    }
    assert_eq!(
        args.last().map(String::as_str),
        Some("https://obsidian.md/help/cli"),
        "args = {args:?}"
    );
    crate::open::page_command("https://id.atlassian.com/manage-profile/security/api-tokens").unwrap();
    assert!(
        crate::open::index_page("https://example.com").is_err(),
        "expected a rejected page"
    );
    assert!(
        crate::open::index_page("https://obsidian.md/help/cli/../download").is_err(),
        "expected a rejected page"
    );
}

#[test]
fn jira_browse_page() {
    use crate::open::jira_browse_page as page_for;
    assert_eq!(
        page_for("https://acme.atlassian.net/", "kiki-12").unwrap(),
        "https://acme.atlassian.net/browse/KIKI-12"
    );
    assert_eq!(
        page_for("https://jira.example.com/jira", "KIKI-12").unwrap(),
        "https://jira.example.com/jira/browse/KIKI-12"
    );
    assert!(page_for("", "KIKI-12").is_err(), "expected a missing site");
    assert!(
        page_for("https://acme.atlassian.net", "not a key").is_err(),
        "expected a rejected key"
    );
    assert!(
        page_for("https://user:pass@acme.atlassian.net", "KIKI-12").is_err(),
        "expected a rejected site"
    );
}

#[test]
fn open_jira_uses_the_configured_browser() {
    let mut app = new_test_server();
    app.store
        .save_atlassian(&mcp::Atlassian {
            jira: mcp::Service {
                url: "https://acme.atlassian.net".into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    app.store
        .create_group(store::GroupInput {
            name: "Billing".into(),
            epic_key: "KIKI-12".into(),
            epic_board_id: "1".into(),
            epic_board_name: "KIKI".into(),
            ..Default::default()
        })
        .unwrap();
    let opened: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let seen = opened.clone();
    app.browser = Arc::new(move |page: &str| {
        *seen.lock().unwrap() = page.to_string();
        Ok(())
    });

    let (status, body) = call(&app, "POST", "/api/jira/open", r#"{"key":"kiki-12"}"#);
    assert_eq!(status, 200, "status = {status} body = {body}");
    assert_eq!(*opened.lock().unwrap(), "https://acme.atlassian.net/browse/KIKI-12");

    let (status, body) = call(&app, "POST", "/api/jira/open", r#"{"key":"KIKI-99"}"#);
    assert_eq!(status, 400, "status = {status} body = {body}");
}
