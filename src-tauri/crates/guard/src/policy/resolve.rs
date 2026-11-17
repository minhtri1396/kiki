//! Picks the agent whose setup applies to a hook's folder.

use std::borrow::Borrow;

use super::model::{Binding, Policy, Query, Resolution};
use super::paths::{abs_path, inside, inside_any};

/// One link of the chain: the resolution it reaches, or `None` to let the next step try.
type Step = fn(&Query, &str) -> Option<Resolution>;

/// Tried in order: a named agent wins, then the agent KIKI started in this worktree,
/// then the only enabled agent that includes the folder.
const STEPS: [Step; 3] = [named_agent, bound_agent, agent_by_folder];

/// Picks the agent whose setup applies to this folder.
pub fn resolve(q: impl Borrow<Query>) -> Resolution {
    let q: &Query = q.borrow();
    let cwd = abs_path(&q.cwd);
    STEPS.iter().find_map(|step| step(q, &cwd)).unwrap_or_default()
}

/// The agent named by `KIKI_AGENT`, when it exists and may use the folder.
fn named_agent(q: &Query, cwd: &str) -> Option<Resolution> {
    let id = q.agent.trim();
    if id.is_empty() {
        return None;
    }
    let Some(policy) = policy_by_id(&q.policies, id) else {
        return Some(Resolution::denied(format!("Guardrail: agent {id} is not available.")));
    };
    if !inside_any(&policy.roots, cwd) && binding_worktree(&q.bindings, id, cwd).is_none() {
        return Some(Resolution::denied(format!(
            "Guardrail: agent {id} is not allowed to use this folder."
        )));
    }
    Some(Resolution::used(match binding_worktree(&q.bindings, id, cwd) {
        Some(binding) => with_binding(policy, binding),
        None => policy,
    }))
}

/// The agent KIKI started in the worktree that holds the folder (the latest one wins).
fn bound_agent(q: &Query, cwd: &str) -> Option<Resolution> {
    let binding = q.bindings.iter().rev().find(|b| inside(&b.worktree, cwd))?;
    let Some(policy) = policy_by_id(&q.policies, &binding.agent_id) else {
        return Some(Resolution::denied(format!(
            "Guardrail: agent {} is not available.",
            binding.agent_id
        )));
    };
    Some(Resolution::used(with_binding(policy, binding)))
}

/// The only agent whose repositories hold the folder. Always ends the chain.
fn agent_by_folder(q: &Query, cwd: &str) -> Option<Resolution> {
    let matched: Vec<&Policy> = q.policies.iter().filter(|p| inside_any(&p.roots, cwd)).collect();
    Some(match matched.as_slice() {
        [only] => Resolution::used((*only).clone()),
        [] if inside_any(&q.workspace, cwd) => {
            Resolution::denied("Guardrail: no agent is set up to use this repository.".to_string())
        }
        [] => Resolution::default(),
        _ => {
            let mut ids: Vec<&str> = matched.iter().map(|p| p.agent_id.as_str()).collect();
            ids.sort();
            Resolution::denied(format!(
                "Guardrail: KIKI cannot tell which agent is running in this repository ({}). Start that agent from KIKI, or set KIKI_AGENT.",
                ids.join(", ")
            ))
        }
    })
}

fn with_binding(policy: Policy, binding: &Binding) -> Policy {
    policy.with_root(&binding.worktree, binding.read_only, &binding.services)
}

fn policy_by_id(list: &[Policy], id: &str) -> Option<Policy> {
    list.iter()
        .find(|p| p.agent_id.to_lowercase() == id.to_lowercase())
        .cloned()
}

fn binding_worktree<'a>(list: &'a [Binding], id: &str, cwd: &str) -> Option<&'a Binding> {
    list.iter()
        .rev()
        .find(|b| b.agent_id.to_lowercase() == id.to_lowercase() && inside(&b.worktree, cwd))
}
