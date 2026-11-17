//! Tests for tasks: ids, create/update/delete, validation, branch names, legacy ids.

use super::*;
use crate::tasks::{encode_ulid, new_task_id};

#[test]
fn encode_ulid_cases() {
    let zero = [0u8; 16];
    assert_eq!(encode_ulid(&zero), "00000000000000000000000000");
    let mut one = [0u8; 16];
    one[15] = 1;
    assert_eq!(encode_ulid(&one), "00000000000000000000000001");
    let mut last = [0u8; 16];
    last[15] = 31;
    assert_eq!(encode_ulid(&last), "0000000000000000000000000Z");
    let mut ms = [0u8; 16];
    ms[5] = 1;
    assert_eq!(encode_ulid(&ms), "00000000010000000000000000");
    assert!(is_ulid(&new_task_id().unwrap()));
}

fn agent(id: &str, status: &str) -> AgentInput {
    AgentInput::new(id, status)
}

fn err_text(err: anyhow::Error) -> String {
    err.to_string()
}

#[test]
fn tasks_round_trip() {
    let (_dir, db) = open_task_store();
    assert!(db.log_file().ends_with("kiki.db"), "log = {}", db.log_file());
    let empty = db.tasks().unwrap();
    assert!(empty.is_empty(), "tasks = {empty:?}");
    let created = db
        .create_task(TaskInput {
            name: "  Ship billing  ".into(),
            description: "Add the invoice page.".into(),
            agents: vec![
                agent("requirement_planner", ""),
                agent("requirement_planner", "doing"),
                agent("backend_runner", "review"),
            ],
            ..Default::default()
        })
        .unwrap();
    assert!(
        is_ulid(&created.id) && created.column == Column::Todo && created.done_at.is_empty(),
        "created = {created:?}"
    );
    assert!(
        created.agents.len() == 2
            && created.agents[0].agent_id == "requirement_planner"
            && created.agents[0].status == AgentStatus::Pending,
        "agents = {:?}",
        created.agents
    );
    assert_eq!(
        created.agents[1].status,
        AgentStatus::Pending,
        "agents = {:?}",
        created.agents
    );
    let loaded = db.task(&created.id).unwrap();
    assert!(
        loaded.name == "Ship billing" && loaded.description == "Add the invoice page.",
        "loaded = {loaded:?}"
    );
    let done = db
        .update_task(
            &created.id,
            TaskInput {
                name: "Ship billing".into(),
                description: "Add the invoice page.".into(),
                column: "done".into(),
                agents: vec![agent("backend_runner", "completed")],
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        done.column == Column::Todo
            && done.done_at.is_empty()
            && done.created_at == created.created_at
            && !done.started,
        "done = {done:?}"
    );
    let kept = db
        .update_task(
            &created.id,
            TaskInput {
                name: "Ship billing".into(),
                column: "done".into(),
                agents: done.agents.iter().map(AgentInput::from).collect(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        kept.column == Column::Todo && kept.done_at.is_empty() && kept.description.is_empty(),
        "kept = {kept:?}"
    );
    let back = db
        .update_task(
            &created.id,
            TaskInput {
                name: "Ship billing".into(),
                column: "progress".into(),
                agents: done.agents.iter().map(AgentInput::from).collect(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        back.column == Column::Todo && back.done_at.is_empty(),
        "back = {back:?}"
    );
    db.delete_task(&created.id).unwrap();
    let err = db.task(&created.id).unwrap_err();
    assert!(
        err.downcast_ref::<ErrTaskNotFound>() == Some(&ErrTaskNotFound),
        "delete err = {err}"
    );
    let err = db.delete_task(&created.id).unwrap_err();
    assert!(is_task_not_found(&err), "second delete err = {err}");
}

#[test]
fn task_branch_slug_survives_edit() {
    let (_dir, db) = open_task_store();
    let created = db
        .create_task(TaskInput {
            name: "Ship billing".into(),
            description: "Add invoices.".into(),
            ..Default::default()
        })
        .unwrap();
    let named = db.set_task_branch(&created.id, "add_cart", &created.name).unwrap();
    assert!(
        named.branch_slug == "add_cart" && named.branch_for == "Ship billing",
        "named = {named:?}"
    );
    let edited = db
        .update_task(
            &created.id,
            TaskInput {
                name: created.name.clone(),
                description: "Add the invoice page.".into(),
                column: "progress".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        edited.branch_slug == "add_cart"
            && edited.branch_for == "Ship billing"
            && edited.description == "Add the invoice page.",
        "edited = {edited:?}"
    );
}

#[test]
fn task_validation() {
    let (_dir, db) = open_task_store();
    let err = db.create_task(TaskInput::default()).unwrap_err();
    assert_eq!(err_text(err), "Enter a task name.");
    let err = db
        .create_task(TaskInput {
            name: "Ship".into(),
            column: "later".into(),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(err_text(err), "Choose TODO, In Progress, or Done.");
    let err = db
        .create_task(TaskInput {
            name: "Ship".into(),
            agents: vec![agent("backend_runner", "paused")],
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(err_text(err), "Agent status is not valid.");
    let long = "a".repeat(4001);
    let err = db
        .create_task(TaskInput {
            name: "Ship".into(),
            description: long,
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(err_text(err), "Description is longer than 4000 characters.");
}

#[test]
fn legacy_numeric_task_id_stays() {
    let dir = tempfile::tempdir().unwrap();
    {
        let raw = rusqlite::Connection::open(dir.path().join("kiki.db")).unwrap();
        raw.execute_batch(
            "CREATE TABLE tasks (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			name TEXT NOT NULL,
			description TEXT NOT NULL,
			column_name TEXT NOT NULL,
			created_at TEXT NOT NULL,
			updated_at TEXT NOT NULL,
			done_at TEXT NOT NULL,
			branch_slug TEXT NOT NULL DEFAULT '',
			branch_for TEXT NOT NULL DEFAULT '',
			started INTEGER NOT NULL DEFAULT 0
		);
		CREATE TABLE task_agents (
			task_id INTEGER NOT NULL,
			agent_id TEXT NOT NULL,
			status TEXT NOT NULL,
			position INTEGER NOT NULL,
			PRIMARY KEY (task_id, agent_id)
		);
		CREATE TABLE task_runs (
			task_id INTEGER NOT NULL,
			agent_id TEXT NOT NULL,
			terminal TEXT NOT NULL,
			orca_status TEXT NOT NULL,
			log TEXT NOT NULL,
			ask_prompt TEXT NOT NULL DEFAULT '',
			ask_options TEXT NOT NULL DEFAULT '[]',
			started_at TEXT NOT NULL,
			updated_at TEXT NOT NULL,
			PRIMARY KEY (task_id, agent_id)
		);
		INSERT INTO tasks (id, name, description, column_name, created_at, updated_at, done_at, started)
			VALUES (1, 'Requirement', 'Manage stores', 'done', '2026-11-16T00:00:00Z', '2026-11-16T00:00:00Z', '2026-11-16T00:00:00Z', 1);
		INSERT INTO task_agents (task_id, agent_id, status, position) VALUES (1, 'requirement_planner', 'completed', 0);
		INSERT INTO task_runs (task_id, agent_id, terminal, orca_status, log, started_at, updated_at)
			VALUES (1, 'requirement_planner', 'term_old', 'idle', 'done', '2026-11-16T00:00:00Z', '2026-11-16T00:00:00Z');",
        )
        .unwrap();
    }
    let db = Store::open(&join(dir.path(), "kiki.yaml")).unwrap();
    let loaded = db.task("1").unwrap();
    assert!(
        loaded.id == "1" && loaded.name == "Requirement" && loaded.started && loaded.column == Column::Done,
        "legacy = {loaded:?}"
    );
    assert!(
        loaded.agents.len() == 1
            && loaded.agents[0].agent_id == "requirement_planner"
            && loaded.agents[0].status == AgentStatus::Completed,
        "agents = {:?}",
        loaded.agents
    );
    assert!(
        loaded.runs.len() == 1 && loaded.runs[0].terminal == "term_old",
        "runs = {:?}",
        loaded.runs
    );
    let created = db
        .create_task(TaskInput {
            name: "Next".into(),
            description: "A new task".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(
        is_ulid(&created.id) && created.id != "1",
        "created id = {:?}",
        created.id
    );
}
