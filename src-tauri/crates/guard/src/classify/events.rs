//! Maps a hook event to the targets its payload touches. Events are the names AI tools use for their hooks.

use super::payload::{agent_name, command, mcp_server, paths_of, tool_name, Obj};
use super::shell::{payload_shell_targets, shell_targets};
use super::tools::{action_kind, first_text, mcp_label, repo_tool_writes, ActionKind};
use crate::policy::{Target, TargetKind};

/// What an event name says about its payload.
enum EventClass {
    Mcp,
    Shell,
    ReadFile,
    EditFile,
    Subagent,
    Tool,
    /// A prompt or model message: nothing is acted on.
    Message,
    Unknown,
}

impl EventClass {
    /// `event` is trimmed and lower case.
    fn of(event: &str) -> EventClass {
        match event {
            "beforemcpexecution" | "aftermcpexecution" => EventClass::Mcp,
            "beforeshellexecution" | "aftershellexecution" => EventClass::Shell,
            "beforereadfile" | "beforetabfileread" => EventClass::ReadFile,
            "beforefileedit" | "afterfileedit" | "beforetabfileedit" | "aftertabfileedit" => EventClass::EditFile,
            "subagentstart" | "subagentstop" => EventClass::Subagent,
            "pretooluse" | "posttooluse" | "posttoolusefailure" | "beforetool" | "aftertool" | "permissionrequest" => {
                EventClass::Tool
            }
            "beforesubmitprompt"
            | "userpromptsubmit"
            | "userpromptexpansion"
            | "beforeagent"
            | "beforemodel"
            | "aftermodel"
            | "afteragentresponse"
            | "afteragentthought"
            | "stop"
            | "sessionstart"
            | "sessionend" => EventClass::Message,
            _ => EventClass::Unknown,
        }
    }
}

/// The targets of a payload for a lower case event name.
pub(super) fn targets(event: &str, m: &Obj) -> Vec<Target> {
    match EventClass::of(event) {
        EventClass::Mcp => vec![Target::named(
            TargetKind::Mcp,
            &mcp_label(&mcp_server(m), &tool_name(m)),
        )],
        EventClass::Shell => payload_shell_targets(m, false),
        EventClass::ReadFile => vec![Target::repo(paths_of(m), false)],
        EventClass::EditFile => vec![Target::repo(paths_of(m), true)],
        EventClass::Subagent => vec![Target::named(TargetKind::Agent, &agent_name(m))],
        EventClass::Tool => tool_targets(m),
        EventClass::Message => Vec::new(),
        EventClass::Unknown => infer(m),
    }
}

/// The targets of a tool call, by what kind of tool it is.
fn tool_targets(m: &Obj) -> Vec<Target> {
    let name = tool_name(m);
    match action_kind(&name) {
        ActionKind::Mcp => {
            vec![Target::named(
                TargetKind::Mcp,
                &mcp_label(&first_text(&[&mcp_server(m), &name]), &name),
            )]
        }
        ActionKind::Agent => vec![Target::named(TargetKind::Agent, &agent_name(m))],
        ActionKind::Repo => payload_shell_targets(m, repo_tool_writes(&name)),
        ActionKind::Tool => vec![Target::named(TargetKind::Tool, &name)],
    }
}

/// One link of the inference chain: the targets it finds, or `None` to let the next rule look.
type InferRule = fn(&Obj) -> Option<Vec<Target>>;

/// For an event this crate does not know, the first rule that recognises the payload decides.
const INFER: [InferRule; 4] = [bare_command, bare_mcp, bare_tool, bare_paths];

fn infer(m: &Obj) -> Vec<Target> {
    INFER.iter().find_map(|rule| rule(m)).unwrap_or_default()
}

/// A shell command that is not a tool or MCP call.
fn bare_command(m: &Obj) -> Option<Vec<Target>> {
    let cmd = command(m);
    (!cmd.is_empty() && tool_name(m).is_empty() && mcp_server(m).is_empty())
        .then(|| shell_targets(&cmd, paths_of(m), false))
}

fn bare_mcp(m: &Obj) -> Option<Vec<Target>> {
    let server = mcp_server(m);
    (!server.is_empty()).then(|| vec![Target::named(TargetKind::Mcp, &server)])
}

fn bare_tool(m: &Obj) -> Option<Vec<Target>> {
    (!tool_name(m).is_empty()).then(|| tool_targets(m))
}

fn bare_paths(m: &Obj) -> Option<Vec<Target>> {
    let paths = paths_of(m);
    (!paths.is_empty()).then(|| vec![Target::repo(paths, false)])
}
