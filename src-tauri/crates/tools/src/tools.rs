use serde::{Deserialize, Serialize};

/// One tool the Settings page can offer.
/// `global` means the switch applies to every agent, with no switch on the agent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spec {
    pub id: String,
    pub name: String,
    pub command: String,
    pub detail: String,
    pub page: String,
    pub global: bool,
}

/// Which tools are turned on for this computer.
/// A missing `guardrails` value means guardrails are on.
///
/// In JSON `github_cli` and `rtk` are always written and `guardrails` is omitted when unset.
/// A YAML writer also omits `false` values itself (see `Settings::empty`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub github_cli: bool,
    #[serde(default)]
    pub rtk: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardrails: Option<bool>,
}

/// GitHub CLI, then RTK.
/// GitHub CLI is chosen on each agent. RTK applies to every agent when it is on.
pub fn catalog() -> Vec<Spec> {
    vec![
        Spec {
            id: "github_cli".into(),
            name: "GitHub CLI".into(),
            command: "gh".into(),
            detail: "The gh command opens pull requests, issues, and checks. Use gh for that work.".into(),
            page: "https://cli.github.com/".into(),
            global: false,
        },
        Spec {
            id: "rtk".into(),
            name: "RTK".into(),
            command: "rtk".into(),
            detail: "The AI tool hook rewrites shell commands through rtk before they run, so the output stays short."
                .into(),
            page: "https://github.com/rtk-ai/rtk".into(),
            global: true,
        },
    ]
}

/// Reports that this id applies to every agent when Settings turns it on.
pub fn global(id: &str) -> bool {
    let id = id.trim();
    for item in catalog() {
        if item.id == id {
            return item.global;
        }
    }
    false
}

impl Settings {
    /// Reports that nothing in this file needs to be stored.
    /// Guardrails stay on when the value is missing, so that choice is not stored.
    pub fn empty(&self) -> bool {
        !self.github_cli && !self.rtk && self.guardrails_on()
    }

    /// Reports that KIKI guardrail hooks should be installed.
    pub fn guardrails_on(&self) -> bool {
        self.guardrails.unwrap_or(true)
    }

    /// Reports that this catalog id is turned on.
    pub fn on(&self, id: &str) -> bool {
        match id.trim() {
            "github_cli" => self.github_cli,
            "rtk" => self.rtk,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_github_cli_then_rtk() {
        let list = catalog();
        assert!(
            list.len() == 2
                && list[0].id == "github_cli"
                && !list[0].global
                && list[1].id == "rtk"
                && list[1].global
                && list[1].command == "rtk",
            "catalog = {list:?}"
        );
        assert!(
            global("rtk") && !global("github_cli") && !global("other"),
            "global flag did not follow the catalog"
        );
        let on = Settings {
            github_cli: true,
            rtk: true,
            guardrails: None,
        };
        assert!(
            !on.empty() && on.on("github_cli") && on.on("rtk") && !on.on("other"),
            "on = {on:?}"
        );
        assert!(
            Settings::default().empty() && Settings::default().guardrails_on(),
            "zero settings should keep guardrails on and store nothing"
        );
        let off = Settings {
            guardrails: Some(false),
            ..Default::default()
        };
        assert!(
            !off.empty() && !off.guardrails_on(),
            "turning guardrails off should be stored"
        );
    }

    #[test]
    fn settings_json_shape() {
        let s = Settings::default();
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"github_cli":false,"rtk":false}"#
        );
        let s = Settings {
            github_cli: true,
            rtk: false,
            guardrails: Some(false),
        };
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"github_cli":true,"rtk":false,"guardrails":false}"#
        );
    }
}
