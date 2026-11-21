pub(crate) const DOCUMENT_WRITER_REPO: &str = r#"This is the only repository you create and edit files in. In the agent document, docs/ means this folder. If the folder is missing or empty, stop and report. Do not create or edit files in the backend, website, or mobile repositories.
The requirement rules are the guide file named for this repository. Do not read a guide file from the repository root.
You write business_requirement.md and product_requirement.md in this folder, and only in the requirement folder the user names. Do not edit other requirement folders, docs/ui-system-design/, or any other file. If another requirement must change, report it. Do not change it.
Saving those files is the update. Do not copy them into another folder, and do not run a sync or index command.
Do not commit, push, or open a pull request unless the user asks."#;

pub(crate) const DESIGNER_REPO: &str = r#"This is the only repository you create and edit files in. In the agent document, docs/ means this folder. If the folder is missing or empty, stop and report. Do not create or edit files in the backend, website, or mobile repositories.
The requirement folder the user names is a path inside this repository. The requirement rules are this folder's AGENTS.md. Do not read an AGENTS.md from the repository root.
You read the requirement and write design.html in that folder."#;

pub(crate) const PLANNER_DOCUMENT_REPO: &str = r#"Read the requirement in this folder: product_requirement.md, business_requirement.md, acceptance_criteria.md when it is present, and design.html. Do not edit any file in this repository. The requirement rules are this folder's AGENTS.md. In the agent document, docs/ means this folder."#;

pub(crate) fn planner_code_repo(kind: &str) -> String {
    format!("Read this repository when the requirement affects {} code. Read its AGENTS.md, and for each service, that service's AGENTS.md. Do not change files, create a branch, or open a pull request here.", kind)
}

pub(crate) fn runner_repo(kind: &str, others: &str) -> String {
    format!(
        r#"This is the only repository you read and change. In the agent document, {}/ means this folder. If the folder is missing or empty, stop and report. Do not read or change the {} repositories.
This folder is the git repository. Run every git and gh command here. Never create a branch, commit, or pull request in the repository root.
It is an Nx monorepo. Each project has its own folder and a project.json. One ticket can touch one or more projects.
The folder rules are this folder's AGENTS.md. The service rules are AGENTS.md inside each service the ticket changes."#,
        kind, others
    )
}

pub(crate) fn reviewer_repo(kind: &str, others: &str) -> String {
    format!(
        r#"This is the only repository you read. In the agent document, {}/ means this folder. If the folder is missing or empty, stop and report. Do not read or change the {} repositories.
This folder is the git repository. Check that .git exists and that git rev-parse --show-toplevel prints this folder. If it does not, stop. Run git and gh here, read-only. Do not commit, push, checkout, or merge.
It is an Nx monorepo. One pull request can touch one or more projects.
The folder rules are this folder's AGENTS.md. The service rules are AGENTS.md inside each service the pull request changes.
Read the feature's product_requirement.md and design.html from the document folder when the ticket or the pull request names that path. Do not edit the document repository."#,
        kind, others
    )
}
