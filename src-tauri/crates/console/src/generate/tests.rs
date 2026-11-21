use std::sync::{Arc, Mutex};

use kiki_util::Ctx;

use super::*;
use crate::error::InputError;
use crate::testing::{fake_run, FakeHost};

fn contains_arg(args: &[String], want: &str) -> bool {
    args.iter().any(|a| a == want)
}

fn contains_pair(args: &[String], name: &str, value: &str) -> bool {
    args.windows(2).any(|w| w[0] == name && w[1] == value)
}

fn request<'a>(tool: AiTool, model: &'a str, kind: &'a str, subject: &'a str, text: &'a str) -> GenerateRequest<'a> {
    GenerateRequest {
        tool,
        model,
        kind,
        subject,
        text,
        context: RewriteContext::default(),
    }
}

fn written(kind: &str, subject: &str, text: &str) -> Result<String> {
    super::rewrite_prompt(kind, subject, text, RewriteContext::default())
}

/// Runs a stream and collects `(command, logs, done)`.
fn collect(host: &FakeHost, req: &GenerateRequest) -> Result<(String, Vec<String>, GenerateEvent)> {
    let mut logs: Vec<String> = Vec::new();
    let mut done = GenerateEvent::default();
    let mut command = String::new();
    generate_stream_with(host, &Ctx::background(), req, &mut |ev| {
        match ev.r#type.as_str() {
            "command" => command = ev.command.clone(),
            "log" => logs.push(format!("{}:{}", ev.channel, ev.text)),
            "done" => done = ev,
            _ => {}
        }
        Ok(())
    })?;
    Ok((command, logs, done))
}

#[test]
fn rewrite_prompt_kinds() {
    let field = written("prompt_field", "requirement_document", "  The Jira key.  ").unwrap();
    assert!(
        field.contains("one field of the prompt for agent requirement_document")
            && field.contains("not the whole prompt")
            && field.ends_with("The Jira key."),
        "field = {field:?}"
    );
    let field = written("prompt_field", "", "The Jira key.").unwrap();
    assert!(
        field.starts_with("This is one field of an agent prompt."),
        "field = {field:?}"
    );
    let input = written("prompt_input", "requirement_document", "## Name\nTicket").unwrap();
    assert!(
        input.contains("one input in the prompt for agent requirement_document")
            && input.contains("Keep every heading")
            && input.ends_with("## Name\nTicket"),
        "input = {input:?}"
    );
    let output = written("prompt_output", "", "## Name\nFolder").unwrap();
    assert!(
        output.starts_with("These are the fields of one output in an agent prompt.")
            && output.contains("A field the user left empty stays empty."),
        "output = {output:?}"
    );
    let prompt = written("prompt", "requirement_document", "  Write the docs.  ").unwrap();
    assert!(
        prompt.contains("prompt for agent requirement_document")
            && prompt.contains("Rewrite the text below in English.")
            && prompt.ends_with("Write the docs."),
        "prompt = {prompt:?}"
    );
    let repo = written("repo", "document", "Keep the vault.").unwrap();
    assert!(repo.contains("document repository"), "repo = {repo:?}");
    let note = written("teamwork", "requirement_designer", "Hand over the folder.").unwrap();
    assert!(note.contains("requirement_designer"), "note = {note:?}");
    let when = written("teamwork_when", "requirement_designer", "files ready").unwrap();
    assert!(
        when.contains("condition under which the agent sends a message to agent requirement_designer")
            && when.ends_with("files ready"),
        "when = {when:?}"
    );
    let reply = written("teamwork_reply", "requirement_designer", "design path").unwrap();
    assert!(
        reply.contains("agent requirement_designer sends back to the agent that gave it work")
            && reply.ends_with("design path"),
        "reply = {reply:?}"
    );
    let mcp = written("mcp", "Atlassian - Jira", "Read the ticket.").unwrap();
    assert!(mcp.contains("Atlassian - Jira MCP server"), "mcp = {mcp:?}");
    let tool = written("tool", "github", "Open the pull request.").unwrap();
    assert!(tool.contains("github tool"), "tool = {tool:?}");
    assert!(written("prompt", "", "   ").is_err(), "empty text was accepted");
    let task = written("task", "", "Thêm giỏ hàng.").unwrap();
    assert!(
        task.contains("task description") && task.contains("4000 characters") && task.ends_with("Thêm giỏ hàng."),
        "task = {task:?}"
    );
    let name = written("task-name", "", "Thêm giỏ hàng").unwrap();
    assert!(
        name.contains("task name") && name.contains("200 characters") && name.ends_with("Thêm giỏ hàng"),
        "name = {name:?}"
    );
    let branch = written("branch", "", "GE-12 thêm giỏ hàng").unwrap();
    assert!(
        branch.contains("git branch")
            && branch.contains("short English name")
            && branch.ends_with("GE-12 thêm giỏ hàng"),
        "branch = {branch:?}"
    );
    assert!(written("other", "", "text").is_err(), "unknown kind was accepted");
}

#[test]
fn coordination_rewrite_includes_the_agents_and_the_message() {
    let shared = RewriteContext {
        prompt: "Write the requirement files.",
        agent: "Document Writer (requirement_document): Writes the requirements.",
        peer: "UI Designer (requirement_designer): Draws the design.",
        when: "The files are ready.",
        send: "Design the folder.",
    };
    let when = super::rewrite_prompt("teamwork_when", "requirement_designer", "files ready", shared).unwrap();
    assert!(
        when.contains("Agent prompt:\nWrite the requirement files.")
            && when.contains("This agent:\nDocument Writer (requirement_document): Writes the requirements.")
            && when.contains("The other agent:\nUI Designer (requirement_designer): Draws the design.")
            && !when.contains("Send when:")
            && when.ends_with("files ready"),
        "when = {when:?}"
    );
    let send = super::rewrite_prompt("teamwork", "requirement_designer", "Hand over the folder.", shared).unwrap();
    assert!(
        send.contains("Send when:\nThe files are ready.")
            && send.contains("Agent prompt:\nWrite the requirement files.")
            && !send.contains("What to send:")
            && send.ends_with("Hand over the folder."),
        "send = {send:?}"
    );
    let reply = super::rewrite_prompt("teamwork_reply", "requirement_designer", "design path", shared).unwrap();
    assert!(
        reply.contains("Send when:\nThe files are ready.")
            && reply.contains("What to send:\nDesign the folder.")
            && reply.contains("This agent:\nDocument Writer")
            && reply.contains("The other agent:\nUI Designer")
            && reply.ends_with("design path"),
        "reply = {reply:?}"
    );
    let prompt = written("prompt", "requirement_document", "Write the docs.").unwrap();
    assert!(
        prompt.contains("Base the rewrite on the user's text.")
            && prompt.contains("must do exactly what the user described")
            && !prompt.contains("Agent prompt:")
            && prompt.ends_with("Write the docs."),
        "prompt = {prompt:?}"
    );
}

#[test]
fn rewrite_prompt_asks_for_a_missing_subject() {
    let cases = [
        ("repo", "Choose a repository first."),
        ("teamwork", "Choose an agent first."),
        ("teamwork_when", "Choose an agent first."),
        ("teamwork_reply", "Choose an agent first."),
        ("mcp", "Choose an MCP server first."),
        ("tool", "Choose a tool first."),
    ];
    for (kind, message) in cases {
        let err = written(kind, " \n ", "text").unwrap_err();
        assert_eq!(err.to_string(), message, "kind {kind}");
        assert!(err.downcast_ref::<InputError>().is_some(), "kind {kind}");
    }
    let err = written("other", "", "text").unwrap_err();
    assert_eq!(
        err.to_string(),
        "Choose a prompt, repository note, Agents Coordination note, MCP note, or tool note."
    );
    let err = written("prompt", "", &"x".repeat(60001)).unwrap_err();
    assert_eq!(err.to_string(), "Text is too long to generate.");
}

#[test]
fn rewrite_prompt_uses_the_subject_on_one_line() {
    let prompt = written("prompt", " planner\n agent ", "Plan.").unwrap();
    assert!(
        prompt.starts_with("This is the prompt for agent planner agent.\n\n"),
        "prompt = {prompt:?}"
    );
    let prompt = written("prompt", "", "Plan.").unwrap();
    assert!(
        prompt.starts_with("This is an agent prompt.\n\n"),
        "prompt = {prompt:?}"
    );
}

#[test]
fn preview_command_labels_the_prompt() {
    let command = preview_command(
        AiTool::Cursor,
        "auto",
        "repo",
        "backend",
        "Ship the API.",
        RewriteContext::default(),
    )
    .unwrap();
    assert!(
        command.contains("cursor-agent --print")
            && command.contains("--workspace {{workspace}} {{prompt}}")
            && !command.contains("Ship the API."),
        "command = {command:?}"
    );
    let gemini = preview_command(
        AiTool::Gemini,
        "",
        "repo",
        "backend",
        "Ship the API.",
        RewriteContext::default(),
    )
    .unwrap();
    assert_eq!(gemini, "gemini --prompt {{prompt}}");
}

#[test]
fn generate_args_uses_print_mode() {
    let (cursor, args) = generate_args(AiTool::Cursor, "composer-2.5", "Rewrite this.").unwrap();
    assert!(
        cursor == "cursor-agent"
            && contains_arg(&args, "--print")
            && contains_arg(&args, "--mode")
            && args.last().unwrap() == "Rewrite this.",
        "cursor = {cursor} {args:?}"
    );
    assert!(
        contains_pair(&args, "--model", "composer-2.5"),
        "model missing: {args:?}"
    );
    let (claude, claude_args) = generate_args(AiTool::Claude, "sonnet", "Rewrite this.").unwrap();
    assert!(
        claude == "claude" && contains_arg(&claude_args, "--print") && contains_arg(&claude_args, "--bare"),
        "claude = {claude} {claude_args:?}"
    );
    let (codex, codex_args) = generate_args(AiTool::Codex, "gpt-5.6-sol", "Rewrite this.").unwrap();
    assert!(
        codex == "codex" && codex_args[0] == "exec" && contains_arg(&codex_args, "--sandbox"),
        "codex = {codex} {codex_args:?}"
    );
    let (gemini, gemini_args) = generate_args(AiTool::Gemini, "gemini-2.5-flash", "Rewrite this.").unwrap();
    assert!(
        gemini == "gemini"
            && contains_pair(&gemini_args, "--prompt", "Rewrite this.")
            && contains_pair(&gemini_args, "--model", "gemini-2.5-flash"),
        "gemini = {gemini} {gemini_args:?}"
    );
    assert!(
        generate_args(AiTool::Cursor, "bad model", "text").is_err(),
        "model with a space was accepted"
    );
}

#[test]
fn every_tool_puts_the_prompt_and_model_where_its_cli_expects() {
    for tool in AiTool::ALL {
        let (bin, with_model) = generate_args(tool, " m1 ", "P").unwrap();
        assert_eq!(bin, tool.binary());
        assert!(contains_pair(&with_model, "--model", "m1"), "{tool}: {with_model:?}");
        let (_, without) = generate_args(tool, "  ", "P").unwrap();
        assert!(!contains_arg(&without, "--model"), "{tool}: {without:?}");
        let last = without.last().unwrap();
        assert_eq!(last, "P", "{tool}: {without:?}");
        let workspace_pos = |args: &[String]| args.iter().position(|a| a == "--workspace");
        let mut args = without.clone();
        add_workspace(tool, &mut args, "/w");
        assert_eq!(
            workspace_pos(&args).is_some(),
            tool == AiTool::Cursor,
            "{tool}: {args:?}"
        );
    }
}

#[test]
fn format_generate_command_quotes_arguments() {
    let args = vec![
        "--print".to_string(),
        "".to_string(),
        "it's here".to_string(),
        "PROMPT".to_string(),
    ];
    assert_eq!(
        format_generate_command("tool", &args, "PROMPT"),
        "tool --print \"\" 'it'\\''s here' {{prompt}}"
    );
}

#[test]
fn generate_stream_logs_then_diff() {
    let got: Arc<Mutex<(String, String, Vec<String>)>> = Arc::new(Mutex::new(Default::default()));
    let stdout = [
        r#"{"type":"thinking","subtype":"delta","text":"Checking"}"#,
        r#"{"type":"thinking","subtype":"completed"}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Clearer"}]}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":" text."}]}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Clearer text."}]}}"#,
        r#"{"type":"result","subtype":"success","result":"Clearer text."}"#,
    ]
    .join("\n");
    let got_in = got.clone();
    let host = FakeHost::new()
        .installed(&["cursor-agent"])
        .starting(move |_, dir, bin, args| {
            *got_in.lock().unwrap() = (dir.to_string(), bin.to_string(), args.to_vec());
            Ok(fake_run(&stdout, "warning\n"))
        });
    let req = request(AiTool::Cursor, "auto", "repo", "backend", "Ship the API.");
    let (command, logs, done) = collect(&host, &req).unwrap();
    let (got_dir, got_bin, got_args) = got.lock().unwrap().clone();
    assert!(
        got_bin == "cursor-agent" && !got_dir.is_empty() && contains_pair(&got_args, "--workspace", &got_dir),
        "ran {got_bin} in {got_dir} with {got_args:?}"
    );
    assert!(
        command.contains("cursor-agent")
            && command.contains(&format!("--workspace {got_dir}"))
            && command.contains("{{prompt}}")
            && !command.contains("Ship the API."),
        "command = {command:?}"
    );
    let joined = logs.join("");
    assert!(
        joined.contains("think:Checking")
            && joined.contains("text:Clearer")
            && joined.contains("text: text.")
            && joined.contains("text:warning"),
        "logs = {logs:?}"
    );
    assert!(
        !joined.contains("text:Clearer text."),
        "the repeated full message was logged again"
    );
    assert_eq!(done.text, "Clearer text.");
    let kinds: Vec<String> = done.diff.iter().map(|l| format!("{}:{}", l.kind, l.text)).collect();
    let found_del = done.diff.iter().any(|l| l.kind == "del" && l.text == "Ship the API.");
    let found_add = done.diff.iter().any(|l| l.kind == "add" && l.text == "Clearer text.");
    assert!(found_del && found_add, "diff = {kinds:?}");
}

#[test]
fn generate_stream_shows_thinking_deltas() {
    let stdout = [
        r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"Look at "}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"the wording."}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Release the API."}}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"Look at the wording."},{"type":"text","text":"Release the API."}]}}"#,
        r#"{"type":"result","subtype":"success","result":"Release the API."}"#,
    ]
    .join("\n");
    let host = FakeHost::new().printing(&stdout, "");
    let (_, logs, _) = collect(
        &host,
        &request(AiTool::Claude, "sonnet", "prompt", "planner", "Ship the API."),
    )
    .unwrap();
    let joined = logs.join("|");
    assert_eq!(
        joined, "think:Look at |think:the wording.|text:Release the API.",
        "logs = {joined:?}"
    );
}

#[test]
fn plain_text_from_a_cli_is_the_answer() {
    let host = FakeHost::new().printing("Better text.\n", "");
    let (_, logs, done) = collect(&host, &request(AiTool::Codex, "", "task", "", "Worse text.")).unwrap();
    assert_eq!(logs, ["text:Better text.\n"]);
    assert_eq!(done.text, "Better text.");
}

#[test]
fn generate_text_returns_the_final_text() {
    let host = FakeHost::new().printing(r#"{"type":"result","result":"```\nFixed.\n```"}"#, "");
    let text = generate_text_with(
        &host,
        &Ctx::background(),
        &request(AiTool::Gemini, "", "task-name", "", "fix"),
    )
    .unwrap();
    assert_eq!(text, "Fixed.");
}

#[test]
fn generate_text_reports_missing_cli() {
    let host = FakeHost::new().installed(&[]);
    let req = request(AiTool::Claude, "sonnet", "prompt", "planner", "Plan the work.");
    let err = generate_text_with(&host, &Ctx::background(), &req).unwrap_err();
    assert!(err.to_string().contains("claude is not installed"), "err = {err}");
    assert!(
        err.downcast_ref::<InputError>().is_none(),
        "a missing CLI was treated as bad input"
    );
}

#[test]
fn bad_input_is_reported_before_the_cli_is_looked_up() {
    let host = FakeHost::new().installed(&[]);
    let err = generate_text_with(
        &host,
        &Ctx::background(),
        &request(AiTool::Claude, "", "prompt", "", " "),
    )
    .unwrap_err();
    assert_eq!(
        err.downcast_ref::<InputError>().unwrap().message,
        "Enter some text first."
    );
    let err = generate_text_with(
        &host,
        &Ctx::background(),
        &request(AiTool::Claude, "bad model", "prompt", "", "x"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "Model is not supported");
}

#[test]
fn a_model_error_result_fails_the_run() {
    let host = FakeHost::new().printing(
        r#"{"type":"result","subtype":"error","is_error":true,"result":"rate  limited"}"#,
        "",
    );
    let err = generate_text_with(&host, &Ctx::background(), &request(AiTool::Claude, "", "task", "", "x")).unwrap_err();
    assert_eq!(err.to_string(), "rate limited");
    let host = FakeHost::new().printing(r#"{"type":"result","subtype":"error","result":""}"#, "");
    let err = generate_text_with(&host, &Ctx::background(), &request(AiTool::Claude, "", "task", "", "x")).unwrap_err();
    assert_eq!(err.to_string(), "the model failed");
}

#[test]
fn a_cli_that_fails_without_output_names_itself() {
    let host = FakeHost::new().starting(|_, _, _, _| {
        let mut run = fake_run("", "boom\n");
        run.wait = Box::new(|| Err(anyhow!("exit status 2")));
        Ok(run)
    });
    let err = generate_text_with(&host, &Ctx::background(), &request(AiTool::Codex, "", "task", "", "x")).unwrap_err();
    assert_eq!(err.to_string(), "codex: exit status 2");
}

#[test]
fn output_wins_over_a_bad_exit() {
    let host = FakeHost::new().starting(|_, _, _, _| {
        let mut run = fake_run("Answer.\n", "");
        run.wait = Box::new(|| Err(anyhow!("exit status 1")));
        Ok(run)
    });
    let text = generate_text_with(&host, &Ctx::background(), &request(AiTool::Codex, "", "task", "", "x")).unwrap();
    assert_eq!(text, "Answer.");
}

#[test]
fn a_silent_cli_returns_no_text() {
    let host = FakeHost::new().printing("", "");
    let err = generate_text_with(&host, &Ctx::background(), &request(AiTool::Codex, "", "task", "", "x")).unwrap_err();
    assert_eq!(err.to_string(), "codex returned no text");
}

#[test]
fn a_deadline_says_the_cli_took_too_long() {
    let host = FakeHost::new().starting(|_, _, _, _| Err(anyhow::Error::new(CtxErr::DeadlineExceeded)));
    let err = generate_text_with(&host, &Ctx::background(), &request(AiTool::Codex, "", "task", "", "x")).unwrap_err();
    assert_eq!(err.to_string(), "codex took too long");
    let host = FakeHost::new().starting(|_, _, _, _| {
        let mut run = fake_run("", "");
        run.wait = Box::new(|| Err(anyhow::Error::new(CtxErr::DeadlineExceeded)));
        Ok(run)
    });
    let err = generate_text_with(&host, &Ctx::background(), &request(AiTool::Codex, "", "task", "", "x")).unwrap_err();
    assert_eq!(err.to_string(), "codex took too long");
}

#[test]
fn a_caller_without_a_deadline_gets_one() {
    let seen: Arc<Mutex<Option<Duration>>> = Arc::new(Mutex::new(None));
    let seen_in = seen.clone();
    let host = FakeHost::new().starting(move |ctx, _, _, _| {
        *seen_in.lock().unwrap() = ctx.remaining();
        Ok(fake_run("ok\n", ""))
    });
    generate_text_with(&host, &Ctx::background(), &request(AiTool::Codex, "", "task", "", "x")).unwrap();
    let remaining = seen.lock().unwrap().expect("a deadline");
    assert!(
        remaining > Duration::from_secs(100) && remaining <= DEFAULT_TIMEOUT,
        "remaining = {remaining:?}"
    );
}

#[test]
fn a_failing_emit_on_the_command_stops_the_run() {
    let host = FakeHost::new().printing("ok\n", "");
    let req = request(AiTool::Codex, "", "task", "", "x");
    let err = generate_stream_with(&host, &Ctx::background(), &req, &mut |_| Err(anyhow!("client gone"))).unwrap_err();
    assert_eq!(err.to_string(), "client gone");
}

#[test]
fn input_error_is_detectable() {
    let err = written("prompt", "", "  ").unwrap_err();
    let input = err.downcast_ref::<InputError>().expect("InputError");
    assert_eq!(input.message, "Enter some text first.");
    assert_eq!(err.to_string(), "Enter some text first.");
}

#[test]
fn generate_event_json_omits_empty_fields() {
    let ev = GenerateEvent {
        r#type: "log".into(),
        channel: "text".into(),
        text: "hi".into(),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_string(&ev).unwrap(),
        r#"{"type":"log","channel":"text","text":"hi"}"#
    );
}

#[test]
fn text_diff_hunk() {
    let diff = text_diff("alpha\nbeta\ngamma\n", "alpha\nBETA\ngamma\ndelta\n");
    assert!(
        !diff.is_empty() && diff[0].kind == "hunk" && diff[0].text.starts_with("@@ "),
        "diff = {diff:?}"
    );
    let got: Vec<String> = diff[1..].iter().map(|l| format!("{}:{}", l.kind, l.text)).collect();
    let want = ["same:alpha", "del:beta", "add:BETA", "same:gamma", "add:delta"];
    assert_eq!(got.join("|"), want.join("|"), "rows = {got:?}");
    assert!(
        text_diff("same\n", "same\n").is_empty(),
        "identical text produced a diff"
    );
}
