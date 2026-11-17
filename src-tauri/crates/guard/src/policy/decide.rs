//! Allows or blocks the targets of a hook event for one agent.

use super::model::{Decision, Policy, Target, TargetKind};
use super::paths::abs_path;

/// What a rule looks at: one target and where the action runs.
pub(super) struct Request<'a> {
    pub cwd: &'a str,
    pub home: &'a str,
    pub target: &'a Target,
}

/// One link of the chain: a block, or `None` to let the next rule look.
type Rule = fn(&Policy, &Request) -> Option<Decision>;

/// A target is blocked by the first rule that objects. Rules for other kinds of target defer.
const RULES: [Rule; 6] = [
    mcp_rule,
    tool_rule,
    agent_rule,
    repo_paths_rule,
    repo_services_rule,
    repo_read_only_rule,
];

impl Policy {
    /// Allows the action only when every target is inside this policy.
    /// An empty target list is an input or output with no MCP, tool, repository, or agent interaction.
    pub fn decide(&self, cwd: &str, home: &str, targets: impl AsRef<[Target]>) -> Decision {
        let cwd = abs_path(cwd);
        for target in targets.as_ref() {
            let request = Request {
                cwd: &cwd,
                home,
                target,
            };
            if let Some(blocked) = RULES.iter().find_map(|rule| rule(self, &request)) {
                return blocked;
            }
        }
        Decision::allow()
    }
}

fn mcp_rule(policy: &Policy, req: &Request) -> Option<Decision> {
    let target = req.target;
    if target.kind != TargetKind::Mcp || policy.allows_mcp(&target.name) {
        return None;
    }
    let label = label_or(&target.name, "an unnamed MCP server");
    Some(Decision::block(format!(
        "Guardrail: this agent cannot use the MCP server {label}."
    )))
}

fn tool_rule(policy: &Policy, req: &Request) -> Option<Decision> {
    let target = req.target;
    if target.kind != TargetKind::Tool {
        return None;
    }
    if target.name.trim().eq_ignore_ascii_case("rtk") {
        return (!policy.rtk).then(|| Decision::block("Guardrail: RTK is off.".to_string()));
    }
    if policy.allows_tool(&target.name) {
        return None;
    }
    let label = label_or(&target.name, "this tool");
    Some(Decision::block(format!(
        "Guardrail: this agent cannot use the tool {label}."
    )))
}

fn agent_rule(policy: &Policy, req: &Request) -> Option<Decision> {
    let target = req.target;
    if target.kind != TargetKind::Agent || policy.allows_agent(&target.name) {
        return None;
    }
    let label = label_or(&target.name, "another agent");
    Some(Decision::block(format!(
        "Guardrail: this agent cannot work with {label}."
    )))
}

fn repo_paths_rule(policy: &Policy, req: &Request) -> Option<Decision> {
    (req.target.kind == TargetKind::Repo).then(|| policy.veto_paths(req.cwd, req.home, &req.target.paths))?
}

fn repo_services_rule(policy: &Policy, req: &Request) -> Option<Decision> {
    (req.target.kind == TargetKind::Repo).then(|| policy.veto_services(req.cwd, req.home, &req.target.paths))?
}

fn repo_read_only_rule(policy: &Policy, req: &Request) -> Option<Decision> {
    (req.target.kind == TargetKind::Repo && req.target.write)
        .then(|| policy.veto_read_only(req.cwd, req.home, &req.target.paths))?
}

/// The trimmed name, or `fallback` when it is blank.
fn label_or(name: &str, fallback: &str) -> String {
    let name = name.trim();
    if name.is_empty() { fallback } else { name }.to_string()
}
