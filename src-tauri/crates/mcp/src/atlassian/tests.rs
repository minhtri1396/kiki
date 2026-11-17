use super::*;
use crate::testutil::sample_atlassian;

#[test]
fn apply_keeps_blank_token() {
    let prev = Atlassian {
        enabled: true,
        username: "a@b.co".into(),
        api_token: "keep-me".into(),
        jira: Service {
            enabled: true,
            url: "https://old.atlassian.net".into(),
            ..Default::default()
        },
        confluence: Service {
            enabled: true,
            url: "https://old.atlassian.net/wiki".into(),
            own: true,
            username: "c@b.co".into(),
            api_token: "other".into(),
            ..Default::default()
        },
    };
    let next = apply(
        &prev,
        &Atlassian {
            enabled: true,
            username: "new@acme.co".into(),
            jira: Service {
                enabled: true,
                url: "https://acme.atlassian.net/".into(),
                ..Default::default()
            },
            confluence: Service {
                enabled: true,
                url: "https://acme.atlassian.net/wiki/".into(),
                own: true,
                username: "c@b.co".into(),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert!(
        next.username == "new@acme.co" && next.api_token == "keep-me" && next.jira.url == "https://acme.atlassian.net",
        "shared = {next:?}"
    );
    assert!(
        next.confluence.url == "https://acme.atlassian.net/wiki"
            && next.confluence.api_token == "other"
            && next.confluence.username == "c@b.co",
        "confluence = {:?}",
        next.confluence
    );
    let cleared = apply(&next, &Atlassian::default());
    assert!(cleared.empty(), "cleared = {cleared:?}");
}

#[test]
fn atlassian_place_round_trip() {
    let cfg = Atlassian {
        enabled: true,
        username: "a@b.co".into(),
        api_token: "secret-token".into(),
        jira: Service {
            enabled: true,
            url: "https://acme.atlassian.net".into(),
            board_id: "12".into(),
            board_name: "KIKI".into(),
            ..Default::default()
        },
        confluence: Service {
            enabled: true,
            url: "https://acme.atlassian.net/wiki".into(),
            space_id: "111".into(),
            space_key: "ENG".into(),
            space_name: "Engineering".into(),
            parent_id: "99".into(),
            parent_title: "Requirements".into(),
            ..Default::default()
        },
    };
    let raw = serde_yaml::to_string(&cfg).unwrap();
    let got: Atlassian = serde_yaml::from_str(&raw).unwrap();
    let got = got.normalized();
    assert!(
        got.jira.board_id == "12"
            && got.jira.board_name == "KIKI"
            && got.confluence.space_key == "ENG"
            && got.confluence.parent_title == "Requirements",
        "got = {got:?}\n{raw}"
    );
}

#[test]
fn validate_rejects_partial_product() {
    let err = Atlassian {
        enabled: true,
        jira: Service {
            enabled: true,
            url: "https://acme.atlassian.net".into(),
            ..Default::default()
        },
        ..Default::default()
    }
    .validate()
    .unwrap_err();
    assert!(err.to_string().contains("Atlassian needs an email"), "err = {err}");
    let err = Atlassian {
        enabled: true,
        jira: Service {
            enabled: true,
            url: "notaurl".into(),
            own: true,
            username: "a@b.co".into(),
            api_token: "t".into(),
            ..Default::default()
        },
        ..Default::default()
    }
    .validate()
    .unwrap_err();
    assert!(err.to_string().contains("http or https"), "err = {err}");
    let off = Atlassian {
        jira: Service {
            url: "https://acme.atlassian.net".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    off.validate().unwrap();
    let ok = Atlassian {
        enabled: true,
        confluence: Service {
            enabled: true,
            url: "https://wiki.example.com".into(),
            own: true,
            username: "a@b.co".into(),
            api_token: "pat".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    ok.validate().unwrap();
}

#[test]
fn legacy_file_uses_shared_credentials() {
    let raw = "
jira:
  url: https://acme.atlassian.net/
  username: a@acme.co
  api_token: secret-jira
confluence:
  url: https://acme.atlassian.net/wiki/
  username: wiki@acme.co
  api_token: secret-wiki
confluence_same_as_jira: false
";
    let cfg: Atlassian = serde_yaml::from_str(raw).unwrap();
    assert!(
        cfg.enabled && cfg.username == "a@acme.co" && cfg.api_token == "secret-jira",
        "shared = {cfg:?}"
    );
    assert!(
        cfg.jira.enabled && !cfg.jira.own && cfg.jira.url == "https://acme.atlassian.net",
        "jira = {:?}",
        cfg.jira
    );
    assert!(
        cfg.confluence.enabled
            && cfg.confluence.own
            && cfg.confluence.username == "wiki@acme.co"
            && cfg.confluence.api_token == "secret-wiki",
        "confluence = {:?}",
        cfg.confluence
    );
    let same = "
jira:
  url: https://acme.atlassian.net
  username: a@acme.co
  api_token: secret-jira
confluence:
  url: https://acme.atlassian.net/wiki
confluence_same_as_jira: true
";
    let cfg: Atlassian = serde_yaml::from_str(same).unwrap();
    assert!(
        cfg.confluence.enabled
            && !cfg.confluence.own
            && cfg.confluence.api_token.is_empty()
            && cfg.api_token == "secret-jira",
        "same = {cfg:?}"
    );
}

#[test]
fn disabled_setup_stays_off() {
    let mut cfg = sample_atlassian();
    cfg.enabled = false;
    let raw = serde_yaml::to_string(&cfg).unwrap();
    let got: Atlassian = serde_yaml::from_str(&raw).unwrap();
    assert!(
        !got.enabled && got.api_token == "secret-jira" && got.jira.enabled && !got.ready(),
        "got = {got:?}\n{raw}"
    );
}

#[test]
fn yaml_keys_keep_the_on_disk_layout() {
    let cfg = Atlassian {
        enabled: true,
        username: "a@b.co".into(),
        jira: Service {
            enabled: true,
            url: "https://x.net".into(),
            own: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let raw = serde_yaml::to_string(&cfg).unwrap();
    assert_eq!(
        raw,
        "enabled: true\nusername: a@b.co\njira:\n  enabled: true\n  url: https://x.net\n  own_credentials: true\n"
    );
    let off = serde_yaml::to_string(&Atlassian::default()).unwrap();
    assert_eq!(off, "enabled: false\n");
}

#[test]
fn env_uses_shared_or_own_credentials() {
    let env = sample_atlassian().env();
    assert_eq!(env["JIRA_URL"], "https://acme.atlassian.net");
    assert_eq!(env["JIRA_API_TOKEN"], "secret-jira");
    assert_eq!(env["CONFLUENCE_API_TOKEN"], "secret-wiki");
    assert!(Atlassian::default().env().is_empty());
}
