use std::os::unix::fs::PermissionsExt;

use super::codex::{toml_basic, toml_has_key};
use super::*;
use crate::testutil::sample_atlassian;

fn with_uvx() -> Installer {
    Installer::with_look_path(Arc::new(|_| Ok("uvx".to_string())))
}

fn without_uvx() -> Installer {
    Installer::with_look_path(Arc::new(|_| Err(anyhow!("file does not exist"))))
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn write_file(path: &Path, data: &str, perm: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, data).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(perm)).unwrap();
}

#[test]
fn ensure_cursor_writes_and_skips() {
    let installer = with_uvx();
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().to_str().unwrap();
    let cfg = sample_atlassian();
    installer.ensure(AiTool::Cursor, "", home, "kiki", &cfg).unwrap();
    let path = dir.path().join(".cursor").join("mcp.json");
    assert_eq!(mode(&path), 0o600);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("\"mcp-atlassian-kiki\"")
            && text.contains("mcp-atlassian")
            && text.contains("secret-jira")
            && text.contains("secret-wiki"),
        "mcp.json =\n{text}"
    );
    let before = text;
    installer.ensure(AiTool::Cursor, "", home, "kiki", &cfg).unwrap();
    let again = std::fs::read_to_string(&path).unwrap();
    assert_eq!(again, before, "second ensure rewrote the file:\n{again}");
}

#[test]
fn ensure_cursor_writes_the_expected_file_format() {
    let installer = with_uvx();
    let dir = tempfile::tempdir().unwrap();
    installer
        .ensure(
            AiTool::Cursor,
            "",
            dir.path().to_str().unwrap(),
            "kiki",
            &sample_atlassian(),
        )
        .unwrap();
    let text = std::fs::read_to_string(dir.path().join(".cursor").join("mcp.json")).unwrap();
    let want = concat!(
        "{\n",
        "  \"mcpServers\": {\"mcp-atlassian-kiki\":{\"args\":[\"mcp-atlassian\"],\"command\":\"uvx\",\"env\":",
        "{\"CONFLUENCE_API_TOKEN\":\"secret-wiki\",\"CONFLUENCE_URL\":\"https://acme.atlassian.net/wiki\",",
        "\"CONFLUENCE_USERNAME\":\"a@acme.co\",\"JIRA_API_TOKEN\":\"secret-jira\",",
        "\"JIRA_URL\":\"https://acme.atlassian.net\",\"JIRA_USERNAME\":\"a@acme.co\"}}}\n",
        "}\n"
    );
    assert_eq!(text, want);
    assert_eq!(mode(&dir.path().join(".cursor")), 0o700);
}

#[test]
fn ensure_cursor_keeps_matching_key() {
    let installer = without_uvx();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".cursor").join("mcp.json");
    let original =
        "{\"mcpServers\":{\"mcp-atlassian-kiki\":{\"command\":\"custom\"},\"jira\":{\"command\":\"other\"}}}\n";
    write_file(&path, original, 0o644);
    installer
        .ensure(
            AiTool::Cursor,
            "",
            dir.path().to_str().unwrap(),
            "kiki",
            &sample_atlassian(),
        )
        .unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn ensure_cursor_adds_workspace_key_beside_other_servers() {
    let installer = with_uvx();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".cursor").join("mcp.json");
    let original =
        "{\"mcpServers\":{\"jira\":{\"command\":\"custom\"},\"mcp-atlassian-other\":{\"command\":\"other\"}}}\n";
    write_file(&path, original, 0o644);
    installer
        .ensure(
            AiTool::Cursor,
            "",
            dir.path().to_str().unwrap(),
            "kiki",
            &sample_atlassian(),
        )
        .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("\"jira\"")
            && text.contains("\"mcp-atlassian-other\"")
            && text.contains("\"mcp-atlassian-kiki\"")
            && text.contains("secret-jira"),
        "mcp.json =\n{text}"
    );
}

#[test]
fn ensure_requires_uvx_before_write() {
    let installer = without_uvx();
    let dir = tempfile::tempdir().unwrap();
    let err = installer
        .ensure(
            AiTool::Gemini,
            "",
            dir.path().to_str().unwrap(),
            "kiki",
            &sample_atlassian(),
        )
        .unwrap_err();
    assert!(err.to_string().contains("uvx is not installed"), "err = {err}");
    assert!(
        !dir.path().join(".gemini").join("settings.json").exists(),
        "wrote Gemini settings without uvx"
    );
}

#[test]
fn ensure_gemini_preserves_other_keys() {
    let installer = with_uvx();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".gemini").join("settings.json");
    let original =
        "{\n  \"theme\": \"dark\",\n  \"mcpServers\": {\"github\":{\"command\":\"echo\",\"args\":[\"hi\"]}}\n}\n";
    write_file(&path, original, 0o644);
    installer
        .ensure(
            AiTool::Gemini,
            "",
            dir.path().to_str().unwrap(),
            "kiki",
            &sample_atlassian(),
        )
        .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("\"theme\": \"dark\"")
            && text.contains("\"command\":\"echo\"")
            && text.contains("\"mcp-atlassian-kiki\""),
        "settings =\n{text}"
    );
}

#[test]
fn ensure_claude_skips_project_server_and_preserves_file() {
    let installer = with_uvx();
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let work_path = work.path().to_str().unwrap();
    let path = home.path().join(".claude.json");
    let original = format!(
        "{{\"num\":1,\"projects\":{{\"{work_path}\":{{\"mcpServers\":{{\"mcp-atlassian-kiki\":{{\"command\":\"uvx\",\"args\":[\"mcp-atlassian\"]}}}}}}}}}}\n"
    );
    write_file(&path, &original, 0o600);
    let home_path = home.path().to_str().unwrap();
    installer
        .ensure(AiTool::Claude, work_path, home_path, "kiki", &sample_atlassian())
        .unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original, "claude.json changed");
    let other = work.path().join("other");
    installer
        .ensure(
            AiTool::Claude,
            other.to_str().unwrap(),
            home_path,
            "kiki",
            &sample_atlassian(),
        )
        .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("\"num\": 1")
            && text.contains("\"mcp-atlassian-kiki\"")
            && text.contains("\"type\":\"stdio\"")
            && text.contains("secret-jira"),
        "claude.json =\n{text}"
    );
}

#[test]
fn ensure_codex_appends() {
    let installer = with_uvx();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".codex").join("config.toml");
    write_file(&path, "model = \"gpt\"\n", 0o644);
    let mut cfg = sample_atlassian();
    cfg.api_token = "tok\"en".into();
    let home = dir.path().to_str().unwrap();
    installer.ensure(AiTool::Codex, "", home, "kiki", &cfg).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.starts_with("model = \"gpt\"\n")
            && text.contains("[mcp_servers.mcp-atlassian-kiki]")
            && text.contains("JIRA_API_TOKEN = \"tok\\\"en\""),
        "config =\n{text}"
    );
    installer.ensure(AiTool::Codex, "", home, "kiki", &cfg).unwrap();
    let again = std::fs::read_to_string(&path).unwrap();
    assert_eq!(again, text, "second ensure rewrote Codex config:\n{again}");
}

#[test]
fn ensure_codex_block_format() {
    let installer = with_uvx();
    let dir = tempfile::tempdir().unwrap();
    installer
        .ensure(
            AiTool::Codex,
            "",
            dir.path().to_str().unwrap(),
            "kiki",
            &sample_atlassian(),
        )
        .unwrap();
    let text = std::fs::read_to_string(dir.path().join(".codex").join("config.toml")).unwrap();
    let want = concat!(
        "[mcp_servers.mcp-atlassian-kiki]\ncommand = \"uvx\"\nargs = [\"mcp-atlassian\"]\n",
        "\n[mcp_servers.mcp-atlassian-kiki.env]\n",
        "CONFLUENCE_API_TOKEN = \"secret-wiki\"\n",
        "CONFLUENCE_URL = \"https://acme.atlassian.net/wiki\"\n",
        "CONFLUENCE_USERNAME = \"a@acme.co\"\n",
        "JIRA_API_TOKEN = \"secret-jira\"\n",
        "JIRA_URL = \"https://acme.atlassian.net\"\n",
        "JIRA_USERNAME = \"a@acme.co\"\n",
    );
    assert_eq!(text, want);
    assert_eq!(mode(&dir.path().join(".codex").join("config.toml")), 0o600);
}

#[test]
fn ensure_skips_when_the_launch_folder_already_has_it() {
    let installer = without_uvx();
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    write_file(
        &work.path().join(".cursor").join("mcp.json"),
        r#"{"mcpServers":{"mcp-atlassian-kiki":{"command":"uvx","args":["mcp-atlassian"]}}}"#,
        0o644,
    );
    installer
        .ensure(
            AiTool::Cursor,
            work.path().to_str().unwrap(),
            home.path().to_str().unwrap(),
            "kiki",
            &sample_atlassian(),
        )
        .unwrap();
    assert!(
        !home.path().join(".cursor").join("mcp.json").exists(),
        "wrote user MCP settings even though the launch folder already has Atlassian"
    );
}

#[test]
fn ensure_empty_and_unknown_tool() {
    let installer = with_uvx();
    let home = tempfile::tempdir().unwrap();
    let home_path = home.path().to_str().unwrap();
    installer
        .ensure(AiTool::Cursor, "", home_path, "kiki", &Atlassian::default())
        .unwrap();
    let mut disabled = sample_atlassian();
    disabled.enabled = false;
    installer
        .ensure(AiTool::Cursor, "", home_path, "kiki", &disabled)
        .unwrap();
    assert!(
        !home.path().join(".cursor").exists(),
        "empty setup created a Cursor folder"
    );
    let err = installer
        .ensure(AiTool::Cursor, "", home_path, "Kiki", &sample_atlassian())
        .unwrap_err();
    assert!(err.to_string().contains("workspace must be lowercase"), "err = {err}");
    assert_eq!(
        AiTool::parse("other"),
        None,
        "the edge rejects an unknown AI tool before ensure runs"
    );
}

#[test]
fn every_tool_has_a_writer_that_requires_uvx_before_writing() {
    let installer = without_uvx();
    for tool in AiTool::ALL {
        let home = tempfile::tempdir().unwrap();
        let err = installer
            .ensure(tool, "", home.path().to_str().unwrap(), "kiki", &sample_atlassian())
            .unwrap_err();
        assert!(err.to_string().contains("uvx is not installed"), "{tool}: err = {err}");
        let written = for_tool(tool).user_config(home.path());
        assert!(!written.exists(), "{tool}: wrote {} without uvx", written.display());
    }
}

#[test]
fn claude_matches_the_launch_folder_after_cleaning_the_path() {
    let installer = without_uvx();
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(".claude.json");
    let original = r#"{"projects":{"/work/app/":{"mcpServers":{"mcp-atlassian-kiki":{}}}}}"#;
    write_file(&path, original, 0o600);
    installer
        .ensure(
            AiTool::Claude,
            "/work/./app/x/..",
            home.path().to_str().unwrap(),
            "kiki",
            &sample_atlassian(),
        )
        .unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn unreadable_settings_are_reported_not_overwritten() {
    let installer = with_uvx();
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(".cursor").join("mcp.json");
    write_file(&path, "[1]", 0o644);
    let err = installer
        .ensure(
            AiTool::Cursor,
            "",
            home.path().to_str().unwrap(),
            "kiki",
            &sample_atlassian(),
        )
        .unwrap_err();
    assert_eq!(err.to_string(), "Cursor MCP settings: not a JSON object");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[1]");
}

#[test]
fn toml_keys_and_values() {
    assert!(toml_has_key(
        "[\"mcp_servers.MCP-Atlassian-Kiki.env\"]\n",
        "mcp-atlassian-kiki"
    ));
    assert!(!toml_has_key(
        "# [mcp_servers.mcp-atlassian-kiki]\n",
        "mcp-atlassian-kiki"
    ));
    assert!(toml_basic("a\nb").is_err());
    assert_eq!(toml_basic("a\\b\"c").unwrap(), "\"a\\\\b\\\"c\"");
}
