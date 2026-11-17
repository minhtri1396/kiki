//! Pattern: Builder. `AgentBuilder` keeps each catalog entry readable: the four identity fields up front,
//! then only the parts that agent has.

use crate::mcps::*;
use crate::peers::*;
use crate::repos::*;
use crate::{Agent, Peer, Repo, Use};
use std::sync::LazyLock;

fn repo(kind: &str, description: impl Into<String>) -> Repo {
    Repo {
        kind: kind.into(),
        description: description.into(),
        ..Default::default()
    }
}

fn peer(id: &str, collaboration: impl Into<String>) -> Peer {
    Peer {
        id: id.into(),
        collaboration: collaboration.into(),
    }
}

/// Builds one catalog [`Agent`].
struct AgentBuilder(Agent);

fn agent(id: &str, title: &str, group: &str, file: &str) -> AgentBuilder {
    AgentBuilder(Agent {
        id: id.into(),
        title: title.into(),
        group: group.into(),
        file: file.into(),
        ..Default::default()
    })
}

impl AgentBuilder {
    fn repos(mut self, repos: Vec<Repo>) -> Self {
        self.0.repos = repos;
        self
    }

    fn peers(mut self, peers: Vec<Peer>) -> Self {
        self.0.peers = peers;
        self
    }

    fn mcps(mut self, mcps: Vec<Use>) -> Self {
        self.0.mcps = mcps;
        self
    }

    /// Ends the chain with the one-line summary.
    fn summary(mut self, summary: &str) -> Agent {
        self.0.summary = summary.into();
        self.0
    }
}

/// One developer agent and its reviewer share a shape; only the team and the other repositories differ.
fn runner(team: &str, title: &str, group: &str, others: &str) -> Agent {
    let (runner_id, reviewer_id) = (format!("{team}_runner"), format!("{team}_reviewer"));
    agent(&runner_id, title, group, &format!("development/{team}/runner.md"))
        .repos(vec![repo(team, runner_repo(team, others))])
        .peers(vec![peer(&reviewer_id, runner_to_reviewer(&runner_id, &reviewer_id))])
        .mcps(atlassian_mcps(RUNNER_JIRA, RUNNER_CONFLUENCE))
        .summary(&format!("Implements a {team} ticket and opens a pull request."))
}

fn reviewer(team: &str, title: &str, group: &str, others: &str) -> Agent {
    let (runner_id, reviewer_id) = (format!("{team}_runner"), format!("{team}_reviewer"));
    agent(&reviewer_id, title, group, &format!("development/{team}/reviewer.md"))
        .repos(vec![repo(team, reviewer_repo(team, others))])
        .peers(vec![peer(&runner_id, reviewer_to_runner(&reviewer_id, &runner_id))])
        .mcps(atlassian_mcps(REVIEWER_JIRA, REVIEWER_CONFLUENCE))
        .summary(&format!("Reviews pull requests from {team}_runner."))
}

/// The launch order shown in the console.
pub static CATALOG: LazyLock<Vec<Agent>> = LazyLock::new(|| {
    vec![
        agent(
            "requirement_document",
            "Document Writer",
            "Requirement",
            "requirement/document.md",
        )
        .repos(vec![repo("document", DOCUMENT_WRITER_REPO)])
        .peers(vec![peer("requirement_designer", DOCUMENT_TO_DESIGNER)])
        .summary("Writes business_requirement.md and product_requirement.md."),
        agent(
            "requirement_designer",
            "UI Designer",
            "Requirement",
            "requirement/designer.md",
        )
        .repos(vec![repo("document", DESIGNER_REPO)])
        .peers(vec![peer("requirement_document", DESIGNER_TO_DOCUMENT)])
        .summary("Draws design.html from the product requirement."),
        agent(
            "requirement_planner",
            "Planner",
            "Requirement",
            "requirement/planner.md",
        )
        .repos(vec![
            repo("document", PLANNER_DOCUMENT_REPO),
            repo("backend", planner_code_repo("backend")),
            repo("website", planner_code_repo("website")),
            repo("mobile", planner_code_repo("mobile")),
        ])
        .peers(vec![
            peer("requirement_document", PLANNER_TO_DOCUMENT),
            peer("requirement_designer", PLANNER_TO_DESIGNER),
            peer("backend_runner", planner_to_runner("backend_runner", "backend")),
            peer("website_runner", planner_to_runner("website_runner", "website")),
            peer("mobile_runner", planner_to_runner("mobile_runner", "mobile")),
        ])
        .mcps(atlassian_mcps(PLANNER_JIRA, PLANNER_CONFLUENCE))
        .summary("Creates Jira tickets from a requirement folder."),
        runner(
            "backend",
            "Senior Backend Developer",
            "Backend",
            "website, mobile, or document",
        ),
        reviewer("backend", "Backend Reviewer", "Backend", "website, mobile, or document"),
        runner(
            "website",
            "Senior Website Developer",
            "Website",
            "backend, mobile, or document",
        ),
        reviewer("website", "Website Reviewer", "Website", "backend, mobile, or document"),
        runner(
            "mobile",
            "Senior Mobile Developer",
            "Mobile",
            "backend, website, or document",
        ),
        reviewer("mobile", "Mobile Reviewer", "Mobile", "backend, website, or document"),
    ]
});
