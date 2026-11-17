//! The task form the web UI sends and the checks that run before a task is saved.

use std::collections::HashSet;

use anyhow::Result;
use kiki_agentfile as agentfile;
use kiki_store as store;
use serde::Deserialize;

use super::error::{null_default, task_error};
use crate::api_error::ApiResult;
use crate::http::{read_json, Request};
use crate::Server;

#[derive(Deserialize, Default)]
#[serde(default)]
struct FormAgent {
    #[serde(rename = "agentId", deserialize_with = "null_default")]
    agent_id: String,
    #[serde(deserialize_with = "null_default")]
    status: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Form {
    #[serde(deserialize_with = "null_default")]
    name: String,
    #[serde(deserialize_with = "null_default")]
    description: String,
    #[serde(deserialize_with = "null_default")]
    column: String,
    #[serde(rename = "groupId", deserialize_with = "null_default")]
    group_id: String,
    #[serde(deserialize_with = "null_default")]
    agents: Vec<FormAgent>,
}

impl Form {
    fn into_input(self) -> store::TaskInput {
        store::TaskInput {
            name: self.name,
            description: self.description,
            column: self.column,
            group_id: self.group_id,
            agents: self
                .agents
                .into_iter()
                .map(|a| store::AgentInput::new(a.agent_id, a.status))
                .collect(),
            ..Default::default()
        }
    }
}

impl Server {
    /// Reads the task form of a request and checks its agents. `id` is empty for a new task, otherwise the
    /// task being edited (its current agents stay allowed even when their agent file is gone).
    pub(crate) fn read_task(&self, req: &Request, id: &str) -> ApiResult<store::TaskInput> {
        let form: Form = read_json(req)?;
        let input = form.into_input();
        self.check_task_agents(id, &input.agents)
            .map_err(|err| task_error(&err))?;
        Ok(input)
    }

    /// Every agent picked for a task must be an agent file of this app, unless the task already has it.
    pub(crate) fn check_task_agents(&self, id: &str, agents: &[store::AgentInput]) -> Result<()> {
        let known = self.known_agent_ids()?;
        let mut kept: HashSet<String> = HashSet::new();
        if !id.is_empty() {
            let current = self.store.task(id)?;
            kept.extend(current.agents.into_iter().map(|agent| agent.agent_id));
        }
        for agent in agents {
            let agent_id = agent.agent_id.trim();
            if agent_id.is_empty() || known.contains(agent_id) || kept.contains(agent_id) {
                continue;
            }
            return Err(store::TaskError::new(format!("{agent_id} is not an agent in this app.")).into());
        }
        Ok(())
    }

    fn known_agent_ids(&self) -> Result<HashSet<String>> {
        let dir = self.agent_dir()?;
        let files = agentfile::list(&dir)?;
        Ok(files.into_iter().map(|file| file.id).collect())
    }
}
