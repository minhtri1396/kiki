//! Tests for board groups and the task link to a group epic.

use super::*;
use crate::groups::GroupRepo;

fn err_text(err: anyhow::Error) -> String {
    err.to_string()
}

#[test]
fn task_group() {
    let (_dir, db) = open_task_store();
    assert_eq!(
        err_text(db.create_group(GroupInput::default()).unwrap_err()),
        "Enter a group name."
    );
    let err = db
        .create_group(GroupInput {
            name: "Billing".into(),
            epic_key: "nope".into(),
            epic_board_id: "12".into(),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(err_text(err), "The Jira epic key is not valid.");
    let local = db
        .create_group(GroupInput {
            name: "Local".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(
        local.epic_key.is_empty() && local.epic_board_id.is_empty() && local.name == "Local",
        "local = {local:?}"
    );
    let group = db
        .create_group(GroupInput {
            name: "Billing".into(),
            description: "The epic.".into(),
            epic_key: "kiki-8".into(),
            epic_board_id: "12".into(),
            epic_board_name: "KIKI".into(),
        })
        .unwrap();
    assert!(
        group.epic_key == "KIKI-8" && group.epic_board_id == "12" && group.name == "Billing" && is_ulid(&group.id),
        "group = {group:?}"
    );
    let task = db
        .create_task(TaskInput {
            name: "Invoice page".into(),
            group_id: group.id.clone(),
            ..Default::default()
        })
        .unwrap();
    assert!(
        task.group_id == group.id
            && task.epic_key == "KIKI-8"
            && task.epic_board_name == "KIKI"
            && !task.epic_owner
            && task.parent_id.is_empty(),
        "task = {task:?}"
    );
    let err = db
        .create_task(TaskInput {
            name: "Missing".into(),
            group_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(err_text(err), "That group was not found.");
    let other = db
        .create_group(GroupInput {
            name: "Accounts".into(),
            epic_key: "KIKI-9".into(),
            epic_board_id: "12".into(),
            epic_board_name: "KIKI".into(),
            ..Default::default()
        })
        .unwrap();
    let moved = db
        .update_task(
            &task.id,
            TaskInput {
                name: "Invoice page".into(),
                group_id: other.id.clone(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        moved.group_id == other.id && moved.epic_key == "KIKI-9",
        "moved = {moved:?}"
    );
    let cleared = db
        .update_task(
            &task.id,
            TaskInput {
                name: "Invoice page".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        cleared.group_id.is_empty() && cleared.epic_key.is_empty() && !cleared.epic_owner,
        "cleared = {cleared:?}"
    );
    let listed = db.groups().unwrap();
    assert!(listed.len() == 3 && listed[0].id == other.id, "groups = {listed:?}");
}

#[test]
fn legacy_epic_becomes_group() {
    let (_dir, db) = open_task_store();
    {
        let guard = db.lock();
        let conn = guard.as_ref().unwrap();
        conn.execute_batch(
            "INSERT INTO tasks (id, name, description, column_name, created_at, updated_at, done_at, parent_id, epic_key, epic_board_id, epic_board_name, epic_owner, group_id)
		VALUES ('1', 'Billing epic', 'The epic.', 'todo', '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z', '', '', 'KIKI-8', '12', 'KIKI', 1, ''),
		       ('2', 'Invoice page', 'A child.', 'todo', '2020-01-02T00:00:00Z', '2020-01-02T00:00:00Z', '', '1', 'KIKI-8', '12', 'KIKI', 0, '')",
        )
        .unwrap();
        GroupRepo::new(conn).migrate_task_groups().unwrap();
    }
    let groups = db.groups().unwrap();
    assert!(
        groups.len() == 1
            && groups[0].name == "Billing epic"
            && groups[0].epic_key == "KIKI-8"
            && groups[0].description == "The epic.",
        "groups = {groups:?}"
    );
    let owner = db.task("1").unwrap();
    let child = db.task("2").unwrap();
    assert!(
        owner.group_id == groups[0].id && !owner.epic_owner && owner.parent_id.is_empty() && owner.epic_key == "KIKI-8",
        "owner = {owner:?}"
    );
    assert!(
        child.group_id == groups[0].id && child.parent_id.is_empty() && child.epic_key == "KIKI-8",
        "child = {child:?}"
    );
}

#[test]
fn delete_group_keeps_task() {
    let (_dir, db) = open_task_store();
    let group = db
        .create_group(GroupInput {
            name: "Billing".into(),
            epic_key: "KIKI-8".into(),
            epic_board_id: "12".into(),
            epic_board_name: "KIKI".into(),
            ..Default::default()
        })
        .unwrap();
    let task = db
        .create_task(TaskInput {
            name: "Invoice page".into(),
            group_id: group.id.clone(),
            ..Default::default()
        })
        .unwrap();
    db.delete_group(&group.id).unwrap();
    db.delete_group(&group.id).unwrap();
    let listed = db.groups().unwrap();
    assert!(
        listed.len() == 1 && listed[0].deleted && listed[0].name == "Billing" && listed[0].epic_key == "KIKI-8",
        "groups = {listed:?}"
    );
    let kept = db.task(&task.id).unwrap();
    assert!(
        kept.group_id == group.id && kept.epic_key == "KIKI-8",
        "task = {kept:?}"
    );
    let saved = db
        .update_task(
            &task.id,
            TaskInput {
                name: "Invoice page".into(),
                group_id: group.id.clone(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        saved.group_id == group.id && saved.epic_key == "KIKI-8",
        "saved = {saved:?}"
    );
    let err = db.delete_group("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap_err();
    assert!(is_group_not_found(&err), "missing err = {err}");
}
