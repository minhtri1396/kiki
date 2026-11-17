use chrono::{DateTime, Utc};
use kiki_orca as orca;
use kiki_store as store;
use serde::Deserialize;

use super::review::review_run;
use super::status::{orca_run_status, RunStatus};
use crate::tasks::test_support::{call, create_task, new_task_test_server, write_script, TaskBody};

fn rfc3339(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[test]
fn orca_run_status_cases() {
    let started = rfc3339(Utc::now() - chrono::Duration::minutes(1));
    assert_eq!(
        orca_run_status("exited", false, &started, Utc::now()),
        RunStatus::Exited
    );
    assert_eq!(
        orca_run_status("running", true, &rfc3339(Utc::now()), Utc::now()),
        RunStatus::Running,
        "fresh idle"
    );
    assert_eq!(orca_run_status("running", true, &started, Utc::now()), RunStatus::Idle);
    assert_eq!(
        orca_run_status("running", false, &started, Utc::now()),
        RunStatus::Running,
        "busy"
    );
    assert_eq!(
        orca_run_status("starting", false, &started, Utc::now()),
        RunStatus::Unknown
    );
    assert_eq!(
        orca_run_status("running", true, "not a time", Utc::now()),
        RunStatus::Running,
        "bad start time"
    );
}

#[test]
fn run_status_texts_are_the_stored_ones() {
    let texts: Vec<&str> = [
        RunStatus::Running,
        RunStatus::Idle,
        RunStatus::Exited,
        RunStatus::Unknown,
    ]
    .into_iter()
    .map(RunStatus::as_str)
    .collect();
    assert_eq!(texts, ["running", "idle", "exited", "unknown"]);
}

#[test]
fn run_phase_follows_the_status_and_the_question() {
    use store::RunPhase::{Doing, Review};
    assert_eq!(RunStatus::Running.phase(false), Some(Doing));
    assert_eq!(RunStatus::Idle.phase(false), Some(Review));
    assert_eq!(RunStatus::Exited.phase(false), Some(Review));
    assert_eq!(RunStatus::Unknown.phase(false), None);
    assert_eq!(
        RunStatus::Running.phase(true),
        Some(Review),
        "a question always waits for review"
    );
    assert_eq!(RunStatus::Unknown.phase(true), Some(Review));
}

#[test]
fn review_run_needs_a_waiting_started_agent() {
    let mut task = store::Task {
        agents: vec![store::TaskAgent {
            agent_id: "a".into(),
            status: store::AgentStatus::Doing,
        }],
        runs: vec![store::TaskRun {
            agent_id: "a".into(),
            terminal: "term_a".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let err = review_run(&task, "a").unwrap_err();
    assert_eq!(err.to_string(), "This agent is not waiting for review.");
    task.agents[0].status = store::AgentStatus::Review;
    assert_eq!(review_run(&task, "a").unwrap().terminal, "term_a");
    task.runs[0].terminal = " ".into();
    assert_eq!(
        review_run(&task, "a").unwrap_err().to_string(),
        "This agent has not been started."
    );
}

#[test]
fn sync_task_runs() {
    let (mut app, tmp) = new_task_test_server();
    let script = tmp.path().join("orca");
    write_script(
        &script,
        r#"#!/bin/sh
if [ "$1" = "terminal" ] && [ "$2" = "read" ]; then
  printf '%s\n' '{"ok":true,"result":{"terminal":{"handle":"term_a","status":"exited","tail":["hello","world"]}}}'
  exit 0
fi
printf '%s\n' '{"ok":false,"error":{"code":"timeout","message":"timeout"}}'
"#,
    );
    app.orca = orca::Client {
        bin: script.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let created = create_task(
        &app,
        r#"{"name":"Ship billing","agents":[{"agentId":"backend_runner","status":"doing"}]}"#,
    );
    app.store
        .save_task_run(&created.id, "backend_runner", "term_a")
        .unwrap();
    let (status, text) = call(&app, "POST", "/api/tasks/sync", None);
    assert_eq!(status, 200, "sync status = {status} body = {text}");
    #[derive(Deserialize)]
    struct Synced {
        tasks: Vec<store::Task>,
    }
    let synced: Synced = serde_json::from_str(&text).unwrap();
    assert!(
        synced.tasks.len() == 1
            && synced.tasks[0].agents.len() == 1
            && synced.tasks[0].agents[0].status == store::AgentStatus::Review,
        "tasks = {:?}",
        synced.tasks
    );
    assert!(
        synced.tasks[0].runs.len() == 1
            && synced.tasks[0].runs[0].status == "exited"
            && synced.tasks[0].runs[0].log == "hello\nworld",
        "runs = {:?}",
        synced.tasks[0].runs
    );
}

#[test]
fn sync_keeps_the_agent_as_it_is_when_orca_cannot_tell() {
    let (mut app, tmp) = new_task_test_server();
    let script = tmp.path().join("orca");
    write_script(
        &script,
        r#"#!/bin/sh
if [ "$1" = "terminal" ] && [ "$2" = "read" ]; then
  printf '%s\n' '{"ok":true,"result":{"terminal":{"handle":"term_a","status":"starting","tail":["booting"]}}}'
  exit 0
fi
printf '%s\n' '{"ok":false,"error":{"code":"timeout","message":"timeout"}}'
"#,
    );
    app.orca = orca::Client {
        bin: script.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let created = create_task(
        &app,
        r#"{"name":"Ship billing","agents":[{"agentId":"backend_runner"}]}"#,
    );
    app.store
        .save_task_run(&created.id, "backend_runner", "term_a")
        .unwrap();
    let asking = [String::from("Short brief")];
    let update = store::RunUpdate::new(&created.id, "backend_runner", "idle", store::RunPhase::Review)
        .log("Thinking")
        .ask("Which one?", &asking);
    app.store.record_task_run(&update).unwrap();

    let (status, text) = call(&app, "POST", "/api/tasks/sync", None);
    assert_eq!(status, 200, "sync status = {status} body = {text}");
    let task = app.store.task(&created.id).unwrap();
    assert_eq!(
        task.agents[0].status,
        store::AgentStatus::Review,
        "agent = {:?}",
        task.agents
    );
    assert!(
        task.runs[0].status == "unknown"
            && task.runs[0].ask_prompt == "Which one?"
            && task.runs[0].ask_options == asking,
        "run = {:?}",
        task.runs[0]
    );
}

#[test]
fn task_reply_and_approve() {
    let (mut app, tmp) = new_task_test_server();
    let log_path = tmp.path().join("args");
    let script = tmp.path().join("orca");
    write_script(
        &script,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\nprintf '%s\\n' '{{\"ok\":true}}'\n",
            log_path.display()
        ),
    );
    app.orca = orca::Client {
        bin: script.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let created = create_task(
        &app,
        r#"{"name":"Ship billing","agents":[{"agentId":"backend_runner","status":"doing"}]}"#,
    );
    let id = created.id.clone();
    app.store
        .save_task_run(&created.id, "backend_runner", "term_a")
        .unwrap();
    let reply_path = format!("/api/tasks/{id}/agents/backend_runner/reply");
    let (status, text) = call(&app, "POST", &reply_path, Some(r#"{"text":"keep going"}"#));
    assert_eq!(status, 400, "early reply status = {status} body = {text}");
    let options = ["Short brief".to_string(), "Full specification".to_string()];
    app.store
        .record_task_run(
            &store::RunUpdate::new(&created.id, "backend_runner", "idle", store::RunPhase::Review)
                .log("Thinking\nTwo shapes fit this task.")
                .ask("Which one should I write?", &options),
        )
        .unwrap();
    let (status, text) = call(&app, "POST", &reply_path, Some(r#"{"text":"Short brief"}"#));
    assert_eq!(status, 200, "reply status = {status} body = {text}");
    let replied = serde_json::from_str::<TaskBody>(&text).unwrap().task;
    assert!(
        replied.agents[0].status == store::AgentStatus::Doing
            && replied.runs[0].ask_prompt.is_empty()
            && replied.runs[0].status == "running",
        "replied = {replied:?}"
    );
    let sent = std::fs::read_to_string(&log_path).unwrap();
    assert!(sent.contains("--text Short brief --enter"), "sent = {sent:?}");
    app.store
        .record_task_run(
            &store::RunUpdate::new(&created.id, "backend_runner", "exited", store::RunPhase::Review)
                .log("I wrote the short brief."),
        )
        .unwrap();
    let (status, text) = call(
        &app,
        "POST",
        &format!("/api/tasks/{id}/agents/backend_runner/approve"),
        None,
    );
    assert_eq!(status, 200, "approve status = {status} body = {text}");
    let approved = serde_json::from_str::<TaskBody>(&text).unwrap().task;
    assert_eq!(
        approved.agents[0].status,
        store::AgentStatus::Completed,
        "approved = {:?}",
        approved.agents
    );
}

#[test]
fn an_empty_reply_is_refused() {
    let (app, _tmp) = new_task_test_server();
    let created = create_task(
        &app,
        r#"{"name":"Ship billing","agents":[{"agentId":"backend_runner"}]}"#,
    );
    let path = format!("/api/tasks/{}/agents/backend_runner/reply", created.id);
    let (status, text) = call(&app, "POST", &path, Some(r#"{"text":"  "}"#));
    assert_eq!((status, text.trim()), (400, r#"{"error":"Enter a reply."}"#));
}
