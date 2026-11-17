# KIKI

This repository is indexed with CodeGraph at the repository root (`.codegraph/`). Query that index before you search or edit code. Sync it after you change code.

## Before you change code

Query the index before grep, find, or reading the repository. Run these from the repository root:

```sh
codegraph explore -p . "<question>"
codegraph callers -p . <symbol>
codegraph callees -p . <symbol>
codegraph impact -p . <symbol>
codegraph affected -p . <files>
```

MCP: `codegraph_explore` with `projectPath` set to this repository and the question in `query`. Use `callers`, `callees`, and `impact` the same way when you need one symbol's trail or blast radius.

How to choose:

- How something works, or which symbols to edit: `explore`.
- Who calls a symbol you are about to change: `callers`, then `impact`.
- What a function calls: `callees`.
- Which tests a change touches: `affected`.

Open a file only when the result marks it stale, when the graph does not cover it (schemas, generated code, fixtures, config, assets), or when a task already names the path. Do not read the whole repository.

CodeGraph is set up when `.codegraph/` exists and `codegraph status .` does not report "not initialized". Do not run `codegraph init`. Do not install CodeGraph. If it is not set up, ask the user once and stop.

## After you change code

The index watches the working tree. Before the next query, and before you finish, run `codegraph status .`. If it reports pending changes, run:

```sh
codegraph sync .
```

Commit `.codegraph/.gitignore` only. Leave the database, sockets, and logs untracked.

## AI tools

Cursor, Claude, Codex, and Gemini are the AI tools an agent can run. Adding another tool means doing all of the following in code before that agent can start. Do not enforce any of it by telling the model in the launch prompt.

1. MCP. Write every MCP that agent selected into that AI tool's own MCP settings when the key is missing. The key is `mcp-<mcp name>-<workspace id>`, for example `mcp-atlassian-kiki`. One Atlassian server covers Jira and Confluence. If a selected MCP is not set up in Settings, do not start the agent.
2. Tools. Every tool that agent selected must be turned on in Settings for that AI tool. If it is not, do not start the agent.
3. Guardrails. Install command hooks on that AI tool for both sides of a turn: before the prompt is sent to the model, and after the model output, including before a tool, MCP, shell, file, or other agent runs. The hook calls this app. Allow or deny is Rust code in `crates/guard`, using the MCP servers, tools, repositories, and agents saved for that agent. When this app is not running, the hook allows the action. When this app is running, it blocks an action the agent was not given. Do not use a prompt hook for this check.

A new AI tool is not done until its MCP install, tool check, and both hook sides are implemented and covered by a test.

## Layout

Tauri v2 project. `src/` is the web UI (`index.html`, `app.css`, `app.js`, `transport.js`). `src-tauri/` is the Rust side and a Cargo workspace: the `kiki` package (`src-tauri/src`: `cli`, `bootstrap`, `app`, `ipc` for the Tauri command, `http_adapter` for the loopback server, `menu`, `window`) and the libraries in `src-tauri/crates/`. `crates/server` is the API (`Server::handle`), `crates/console` builds launch prompts, `crates/store` is SQLite, `crates/jobs` runs index setup, `crates/guard` holds the guardrail policy and hook installers, `crates/mcp` and `crates/tools` set up MCP servers and CLI tools, `crates/aitool` is the `AiTool` enum, `crates/agents` holds the embedded role files. Name those crates in the query instead of walking the tree.

## Verify

Run from `src-tauri/`:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
