//! Deciding one hook event: find the agent whose setup applies, then let its policy allow or block the action.

use std::collections::HashSet;

use anyhow::Result;
use kiki_agentfile as agentfile;
use kiki_guard as kg;

use super::policy::{policy_from, repo_folders};
use crate::{tools_from, Server};

const NO_SETUP: &str = "Guardrail: KIKI could not read the agent setup, so this action is blocked.";

impl Server {
    /// The answer to one hook: `agent` is the agent the AI tool says it is (empty when it does not say),
    /// `cwd` the folder it works in, `event` the hook name and `input` the hook payload.
    pub(crate) fn check_guard(&self, agent: &str, cwd: &str, event: &str, input: &[u8]) -> kg::Decision {
        let Ok(mut query) = self.guard_query() else {
            return kg::Decision {
                allow: false,
                message: NO_SETUP.to_string(),
            };
        };
        query.agent = agent.to_string();
        query.cwd = cwd.to_string();
        let resolved = kg::resolve(&query);
        match resolved.status {
            kg::Status::Outside => kg::Decision {
                allow: true,
                message: String::new(),
            },
            kg::Status::Deny => kg::Decision {
                allow: false,
                message: resolved.message,
            },
            kg::Status::Use => resolved
                .policy
                .decide(cwd, &self.decision_home(), kg::classify(event, input)),
        }
    }

    /// The home folder paths are compared against: the configured one, or `$HOME` (empty when it is not set).
    fn decision_home(&self) -> String {
        if self.mcp_home.is_empty() {
            std::env::var("HOME").unwrap_or_default()
        } else {
            self.mcp_home.clone()
        }
    }

    /// Everything the guardrail needs to pick an agent: the running bindings, the policy of each enabled agent
    /// and the repository folders of the workspace.
    fn guard_query(&self) -> Result<kg::Query> {
        let snap = self.load()?;
        let files = agentfile::list(&self.agent_dir()?)?;
        let bindings = self.guard.lock().unwrap_or_else(|e| e.into_inner()).bindings.clone();
        let cfg = self.load_tools()?;
        let enabled: HashSet<String> = tools_from(&cfg).into_iter().map(|item| item.id).collect();
        let policies = files
            .iter()
            .filter(|file| file.enabled)
            .map(|file| policy_from(file, &snap, &enabled, cfg.rtk))
            .collect();
        Ok(kg::Query {
            bindings,
            policies,
            workspace: repo_folders(&snap),
            ..Default::default()
        })
    }
}
