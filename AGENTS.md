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

## Layout

Go module `kiki`. `main.go` starts the desktop app. `internal/server` is the HTTP API and the web UI in `internal/server/web`. `internal/console` builds launch prompts. `internal/store` is SQLite. `internal/jobs` runs index setup. `internal/agents` holds the embedded role files. Name those packages in the query instead of walking the tree.

## Verify

```sh
go test ./...
```
