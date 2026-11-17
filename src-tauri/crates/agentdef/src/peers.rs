pub(crate) const DOCUMENT_TO_DESIGNER: &str = r#"requirement_designer turns the requirement folder into design.html. Do not write design.html. When business_requirement.md and product_requirement.md are ready, tell the user to run agent id requirement_designer on that folder. In the report, give the agent id and the folder path."#;

pub(crate) const DESIGNER_TO_DOCUMENT: &str = r#"requirement_document writes business_requirement.md and product_requirement.md in the folder the user gives you. Read those files. Do not edit them. If a behavior, a state, or a rule you need is missing, ask the user once. Do not message agent id requirement_document."#;

pub(crate) const PLANNER_TO_DOCUMENT: &str = r#"requirement_document writes business_requirement.md and product_requirement.md. Read those files. Do not edit them, and do not message agent id requirement_document. If the requirement is missing or ambiguous, ask the user."#;

pub(crate) const PLANNER_TO_DESIGNER: &str = r#"requirement_designer writes design.html, or design.admin-web.html and design.user-app.html. Read the design. Do not edit it, and do not message agent id requirement_designer. If there is no design and the requirement has website or mobile work, ask the user whether to plan from the requirement only or to wait for requirement_designer."#;

pub(crate) fn planner_to_runner(id: &str, team: &str) -> String {
    format!("After the user reviews the tickets, the user gives each {} Task to agent id {}. Do not message {}. Write the Task so {} can implement from the ticket alone.", team, id, id, id)
}

pub(crate) fn runner_to_reviewer(runner: &str, reviewer: &str) -> String {
    format!(
        r#"{} reviews every PR {} opens and says when a sub-task PR may be merged. Send the review request to the terminal titled {}. Start the message with [{} -> {}]. After you send it, end your turn. The reply arrives later and starts with [{} -> {}].
Changes requested: fix the same sub-task branch, push, and tell {} it is ready for re-review.
LGTM on a sub-task PR: merge that PR into the parent branch only after {} explicitly approves it.
LGTM on the parent PR: do not merge it. Tell the user the parent PR is approved and ready for them to merge.
A message from the user in your terminal takes precedence over {}."#,
        reviewer, runner, reviewer, runner, reviewer, reviewer, runner, reviewer, reviewer, reviewer
    )
}

pub(crate) fn reviewer_to_runner(reviewer: &str, runner: &str) -> String {
    format!(
        r#"{} implements the code and opens the PRs {} reviews. Send the result to the terminal titled {}. Start the message with [{} -> {}]. Include the ticket key, the PR link, and what to do: merge, or update the code. After you send it, end your turn. The reply arrives later and starts with [{} -> {}].
The user merges the parent PR. You do not merge. When the parent PR is approved, tell the user the outcome, the PR link, and that they merge it."#,
        runner, reviewer, runner, reviewer, runner, runner, reviewer
    )
}
