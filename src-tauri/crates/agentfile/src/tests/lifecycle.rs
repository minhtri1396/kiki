use super::*;

use std::path::Path;

#[test]
fn new_id_joins_snake_case_category_and_name() {
    assert_eq!(new_id("Requirement", "Document Writer"), "requirement_document_writer");
    assert_eq!(new_id("support team", "QA Lead"), "support_team_qa_lead");
    assert_eq!(new_id("documentWriter", "API"), "document_writer_api");
    assert_eq!(new_id("support", "!!!"), "");
    assert_eq!(new_id("", "Runner"), "");
}

#[test]
fn create_writes_a_new_file_and_rejects_a_taken_id() {
    let td = tmp();
    let dir = agents_dir(&td);
    ensure(&dir, "cursor").unwrap();
    let current = find_ok(&dir, "backend_runner");
    let mut next = current.clone();
    next.id = "support_runner".into();
    next.name = "Support".into();
    next.category = "support".into();
    next.description = "Answers support questions.".into();
    next.works_with.clear();
    next.path.clear();
    create(&dir, &next).unwrap();
    let saved = find_ok(&dir, "support_runner");
    assert_eq!(saved.name, "Support");
    assert_eq!(saved.category, "support");
    assert!(Path::new(&dir).join("support").join("support_runner.md").is_file());
    let err = create(&dir, &next).unwrap_err();
    assert!(is_exists(&err), "err = {err}");
}

#[test]
fn delete_removes_the_file_and_collaborators() {
    let td = tmp();
    let dir = agents_dir(&td);
    ensure(&dir, "cursor").unwrap();
    let before = find_ok(&dir, "requirement_document");
    assert!(before.works_with.iter().any(|peer| peer.id == "requirement_designer"));
    delete(&dir, "requirement_designer").unwrap();
    assert!(find(&dir, "requirement_designer").unwrap().is_none());
    let after = find_ok(&dir, "requirement_document");
    assert!(after.works_with.iter().all(|peer| peer.id != "requirement_designer"));
    let err = delete(&dir, "requirement_designer").unwrap_err();
    assert!(is_not_found(&err), "err = {err}");
}

#[test]
fn delete_removes_an_empty_category_folder() {
    let td = tmp();
    let dir = agents_dir(&td);
    ensure(&dir, "cursor").unwrap();
    let current = find_ok(&dir, "backend_runner");
    let mut next = current.clone();
    next.id = "support_runner".into();
    next.category = "support".into();
    next.works_with.clear();
    create(&dir, &next).unwrap();
    delete(&dir, "support_runner").unwrap();
    assert!(!Path::new(&dir).join("support").exists());
}
