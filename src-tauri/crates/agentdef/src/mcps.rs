use crate::Use;

pub(crate) const PLANNER_JIRA: &str = r#"Search Jira before you create a ticket. Create and update the Epic, Stories, Tasks, and Subtasks for one requirement folder. Read issue types, fields, options, link types, and transitions from Jira. Do not invent them. Do not call Jira until the user gives a requirement folder."#;

pub(crate) const RUNNER_JIRA: &str = r#"Read the parent ticket the user gives you, and its subtasks. Do not search Jira for work. Move a ticket only to a status named in the setup file, after you read its transitions. Comment on the ticket when that transition is not available."#;

pub(crate) const REVIEWER_JIRA: &str = r#"Read the ticket for the pull request. For a subtask, also read the parent ticket. Move that ticket to In Review before you review, after you read its transitions. Comment on the ticket when that transition is not available. Do not set a ticket to Done."#;

pub(crate) const PLANNER_CONFLUENCE: &str =
    r#"Read a Confluence page when the requirement or the user names it. Do not create, edit, or delete a page."#;

pub(crate) const RUNNER_CONFLUENCE: &str =
    r#"Read a Confluence page when the ticket or the user names it. Do not create, edit, or delete a page."#;

pub(crate) const REVIEWER_CONFLUENCE: &str = r#"Read a Confluence page when the ticket, the pull request, or the user names it. Do not create, edit, or delete a page."#;

pub(crate) fn atlassian_mcps(jira: &str, confluence: &str) -> Vec<Use> {
    vec![
        Use {
            id: "atlassian_jira".into(),
            description: jira.into(),
            ..Default::default()
        },
        Use {
            id: "atlassian_confluence".into(),
            description: confluence.into(),
            ..Default::default()
        },
    ]
}
