//! The Agent Orchestration tools a user can choose. Only Orca is supported.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

/// One app that starts agents and groups their work.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Orchestrator {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "label")]
    pub label: String,
}

/// The Agent Orchestration tools a user can choose.
/// Only Orca is supported.
pub static ORCHESTRATORS: LazyLock<Vec<Orchestrator>> = LazyLock::new(|| {
    vec![Orchestrator {
        id: "orca".into(),
        label: "Orca".into(),
    }]
});

/// The saved tool, or Orca when none is set.
pub fn normalize_orchestrator(id: &str) -> String {
    let id = id.trim();
    if id.is_empty() {
        return "orca".to_string();
    }
    id.to_string()
}

/// Reports whether `id` is a supported Agent Orchestration tool.
/// An empty id is Orca.
pub fn known_orchestrator(id: &str) -> bool {
    let id = normalize_orchestrator(id);
    ORCHESTRATORS.iter().any(|item| item.id == id)
}

/// The name shown for an Agent Orchestration tool.
pub fn orchestrator_label(id: &str) -> String {
    let id = normalize_orchestrator(id);
    match ORCHESTRATORS.iter().find(|item| item.id == id) {
        Some(item) => item.label.clone(),
        None => id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_id_is_orca() {
        assert_eq!(normalize_orchestrator("  "), "orca");
        assert!(known_orchestrator(""));
        assert!(!known_orchestrator("other"));
        assert_eq!(orchestrator_label(""), "Orca");
        assert_eq!(orchestrator_label(" other "), "other");
    }
}
