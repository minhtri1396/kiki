//! Tests for the task lifecycle: runs, agent status transitions and the board column.

use super::*;

fn agent(id: &str, status: &str) -> AgentInput {
    AgentInput::new(id, status)
}

fn err_text(err: anyhow::Error) -> String {
    err.to_string()
}

#[test]
fn task_run_follows_agent() {
    let (_dir, db) = open_task_store();
    let created = db
        .create_task(TaskInput {
            name: "Ship billing".into(),
            agents: vec![agent("backend_runner", "doing")],
            ..Default::default()
        })
        .unwrap();
    db.save_task_run(&created.id, "backend_runner", "term_a").unwrap();
    let loaded = db.task(&created.id).unwrap();
    assert!(
        loaded.started
            && loaded.column == Column::Progress
            && loaded.runs.len() == 1
            && loaded.runs[0].terminal == "term_a"
            && loaded.runs[0].status == "running"
            && loaded.runs[0].log.is_empty(),
        "loaded = {loaded:?}"
    );
    let open = db.open_runs().unwrap();
    assert!(open.len() == 1 && open[0].task_id == created.id, "open = {open:?}");
    let finished = db
        .record_task_run(
            &RunUpdate::new(&created.id, "backend_runner", "exited", RunPhase::Review)
                .log("I added the invoice route."),
        )
        .unwrap();
    assert!(
        finished.agents[0].status == AgentStatus::Review
            && finished.runs[0].status == "exited"
            && finished.runs[0].log == "I added the invoice route.",
        "finished = {finished:?}"
    );
    let approved = db.approve_agent(&created.id, "backend_runner").unwrap();
    assert!(
        approved.agents[0].status == AgentStatus::Completed
            && approved.column == Column::Done
            && !approved.done_at.is_empty(),
        "approved = {approved:?}"
    );
    assert!(
        db.approve_agent(&created.id, "backend_runner").is_err(),
        "second approve succeeded"
    );
    let open = db.open_runs().unwrap();
    assert!(open.is_empty(), "open = {open:?}");
    let err = db
        .update_task(
            &created.id,
            TaskInput {
                name: "Renamed".into(),
                column: "todo".into(),
                agents: vec![agent("requirement_planner", "pending")],
                ..Default::default()
            },
        )
        .unwrap_err();
    assert_eq!(err_text(err), "This task has already started.");
    let kept = db.task(&created.id).unwrap();
    assert!(
        kept.name == "Ship billing"
            && kept.runs.len() == 1
            && kept.agents[0].agent_id == "backend_runner"
            && kept.agents[0].status == AgentStatus::Completed,
        "kept = {kept:?}"
    );
    db.delete_task(&created.id).unwrap();
    let open = db.open_runs().unwrap();
    assert!(open.is_empty(), "open after delete = {open:?}");
}

fn started_task(db: &Store, status: &str) -> Task {
    let created = db
        .create_task(TaskInput {
            name: "Ship billing".into(),
            agents: vec![agent("backend_runner", status)],
            ..Default::default()
        })
        .unwrap();
    db.save_task_run(&created.id, "backend_runner", "term_a").unwrap();
    db.task(&created.id).unwrap()
}

#[test]
fn starting_a_run_marks_the_agent_doing_and_the_task_started() {
    let (_dir, db) = open_task_store();
    let task = started_task(&db, "pending");
    assert_eq!(task.agents[0].status, AgentStatus::Doing);
    assert!(task.started);
    assert_eq!(task.column, Column::Progress);
}

#[test]
fn run_updates_follow_the_phase_and_never_undo_a_completed_agent() {
    let (_dir, db) = open_task_store();
    let task = started_task(&db, "pending");
    let options = vec!["yes".to_string(), "no".to_string()];
    let waiting = db
        .record_task_run(
            &RunUpdate::new(&task.id, "backend_runner", "idle", RunPhase::Review).ask("Ship it?", &options),
        )
        .unwrap();
    assert_eq!(waiting.agents[0].status, AgentStatus::Review);
    assert_eq!(waiting.runs[0].ask_prompt, "Ship it?");
    assert_eq!(waiting.runs[0].ask_options, options);
    let working = db
        .record_task_run(&RunUpdate::new(&task.id, "backend_runner", "running", RunPhase::Doing))
        .unwrap();
    assert_eq!(working.agents[0].status, AgentStatus::Doing);
    assert_eq!(working.runs[0].ask_prompt, "", "a working agent has no question");
    assert!(working.runs[0].ask_options.is_empty());
    db.record_task_run(&RunUpdate::new(&task.id, "backend_runner", "idle", RunPhase::Review))
        .unwrap();
    db.approve_agent(&task.id, "backend_runner").unwrap();
    let after = db
        .record_task_run(&RunUpdate::new(&task.id, "backend_runner", "running", RunPhase::Doing))
        .unwrap();
    assert_eq!(
        after.agents[0].status,
        AgentStatus::Completed,
        "completed stays completed until a new start"
    );
    assert_eq!(after.column, Column::Done);
}

#[test]
fn record_task_run_checks_its_input_and_the_run() {
    let (_dir, db) = open_task_store();
    let task = started_task(&db, "pending");
    let err = db
        .record_task_run(&RunUpdate::new(&task.id, "  ", "idle", RunPhase::Review))
        .unwrap_err();
    assert_eq!(err_text(err), "Orca did not return a terminal for the agent.");
    let err = db
        .record_task_run(&RunUpdate::new(&task.id, "someone_else", "idle", RunPhase::Review))
        .unwrap_err();
    assert!(is_task_not_found(&err), "err = {err}");
    let err = db.save_task_run("missing", "backend_runner", "term").unwrap_err();
    assert!(is_task_not_found(&err), "err = {err}");
}

#[test]
fn resume_and_approve_need_an_agent_waiting_for_review() {
    let (_dir, db) = open_task_store();
    let task = started_task(&db, "pending");
    assert_eq!(
        err_text(db.resume_agent(&task.id, "backend_runner").unwrap_err()),
        "This agent is not waiting for review."
    );
    assert_eq!(
        err_text(db.approve_agent(&task.id, "backend_runner").unwrap_err()),
        "Approve is available when the agent is waiting for review."
    );
    assert_eq!(
        err_text(db.resume_agent(&task.id, "nobody").unwrap_err()),
        "This agent is not waiting for review."
    );
    db.record_task_run(&RunUpdate::new(&task.id, "backend_runner", "idle", RunPhase::Review).log("done?"))
        .unwrap();
    let resumed = db.resume_agent(&task.id, " backend_runner ").unwrap();
    assert_eq!(resumed.agents[0].status, AgentStatus::Doing);
    assert_eq!(resumed.runs[0].status, "running");
    assert_eq!(resumed.column, Column::Progress);
}

#[test]
fn done_column_follows_the_last_approval_and_leaves_when_an_agent_is_added_back() {
    let (_dir, db) = open_task_store();
    let created = db
        .create_task(TaskInput {
            name: "Two agents".into(),
            agents: vec![agent("backend_runner", ""), agent("requirement_planner", "")],
            ..Default::default()
        })
        .unwrap();
    for id in ["backend_runner", "requirement_planner"] {
        db.save_task_run(&created.id, id, "term").unwrap();
        db.record_task_run(&RunUpdate::new(&created.id, id, "idle", RunPhase::Review))
            .unwrap();
    }
    let first = db.approve_agent(&created.id, "backend_runner").unwrap();
    assert_eq!(first.column, Column::Progress, "one agent still waiting");
    assert!(first.done_at.is_empty());
    let second = db.approve_agent(&created.id, "requirement_planner").unwrap();
    assert_eq!(second.column, Column::Done);
    assert!(!second.done_at.is_empty());
    let back = db.resume_agent(&created.id, "backend_runner");
    assert!(back.is_err(), "a completed agent cannot resume");
}

#[test]
fn stored_values_serialize_to_the_same_text() {
    let (_dir, db) = open_task_store();
    let task = started_task(&db, "review");
    let json = serde_json::to_value(&task).unwrap();
    assert_eq!(json["column"], "progress");
    assert_eq!(json["agents"][0]["status"], "doing");
    let raw: (String, String) = db
        .with_db(|conn| {
            Ok((
                conn.query_row("SELECT column_name FROM tasks WHERE id = ?", [&task.id], |row| {
                    row.get(0)
                })?,
                conn.query_row("SELECT status FROM task_agents WHERE task_id = ?", [&task.id], |row| {
                    row.get(0)
                })?,
            ))
        })
        .unwrap();
    assert_eq!(raw, ("progress".to_string(), "doing".to_string()));
}
