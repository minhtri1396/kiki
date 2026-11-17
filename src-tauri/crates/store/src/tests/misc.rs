//! Tests for launches, thread safety and JSON shapes.

use std::fs;

use super::*;

#[test]
fn launches_round_trip_and_migrate_from_setup_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = join(dir.path(), "kiki.yaml");
    fs::write(
        &file,
        "workspace_id: studio\nlaunches:\n    - id: 7\n      agent_id: planner\n      runtime: cursor\n      note: first\n      status: ok\n      detail: d\n      created_at: 2020-01-01T00:00:00Z\n    - id: 0\n      agent_id: skipped\n",
    )
    .unwrap();
    let db = Store::open(&file).unwrap();
    let text = fs::read_to_string(&file).unwrap();
    assert!(!text.contains("launches"), "launches were kept:\n{text}");
    let listed = db.launches(0).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, 7);
    assert_eq!(listed[0].agent_id, "planner");
    let added = db.add_launch("backend", "orca", "note", "ok", "detail").unwrap();
    assert_eq!(added.id, 8);
    let listed = db.launches(10).unwrap();
    assert_eq!(listed.iter().map(|l| l.id).collect::<Vec<_>>(), vec![8, 7]);
    db.delete_launch(7).unwrap();
    let err = db.delete_launch(7).unwrap_err();
    assert!(is_not_found(&err), "err = {err}");
    assert_eq!(err.to_string(), "run was not found");
    db.clear_launches().unwrap();
    assert!(db.launches(10).unwrap().is_empty());
}

#[test]
fn store_is_shared_between_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Store>();
    let (_dir, db) = open_task_store();
    let db = std::sync::Arc::new(db);
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let db = db.clone();
            std::thread::spawn(move || {
                for n in 0..5 {
                    db.create_task(TaskInput {
                        name: format!("task {i}-{n}"),
                        ..Default::default()
                    })
                    .unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(db.tasks().unwrap().len(), 20);
    db.close().unwrap();
    assert!(db.tasks().is_err());
}

#[test]
fn json_shapes_are_stable() {
    let task = Task::default();
    let v = serde_json::to_value(&task).unwrap();
    assert!(v["agents"].is_array() && v["runs"].is_array());
    assert!(v.get("createdAt").is_some() && v.get("epicBoardName").is_some() && v.get("groupId").is_some());
    let run = TaskRun {
        task_id: "x".into(),
        ..Default::default()
    };
    let v = serde_json::to_value(&run).unwrap();
    assert!(v.get("task_id").is_none() && v.get("taskId").is_none());
    assert!(v["askOptions"].is_array());
    let team = serde_json::to_value(TeamFolder::default()).unwrap();
    assert!(team.get("Path").is_some());
    assert_eq!(detail_limit("short"), "short");
    let long = "x".repeat(8005);
    assert!(detail_limit(&long).ends_with("\n… (5 bytes truncated)"));
}
