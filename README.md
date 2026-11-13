# KIKI

KIKI is a desktop console for a multi-agent software process. It defines who does which job, on which repository, and starts those agents together when a feature is ready to implement.

Each agent has one written role. The roles live in `internal/agents/` and are embedded in the KIKI binary at build time. Agents do not share one checkout. Each works in its own repository, and they hand work to each other through the requirement folder and the indexes.

## Repositories

KIKI keeps four repositories as folders next to the binary. On startup it creates any of these that are missing.

| Folder | Kind | Who uses it |
| --- | --- | --- |
| `backend/` | backend | Backend developer and reviewer |
| `website/` | website | Website developer and reviewer |
| `mobile/` | mobile | Mobile developer and reviewer |
| `docs/` | document | Document writer, UI designer, and the planner |

`backend`, `website`, and `mobile` are the code repositories. Each one is its own git repository. `docs` is the document repository: requirements and screen designs live there, and it has no service list. An empty folder can be filled from the Repositories tab with **Clone repository**. The dialog can take a folder path. KIKI then checks out only that folder, plus files at the repository root. Other folders stay on the remote, and commit, push, and pull use that checkout.

A code repository can hold one project or several. When it holds several projects, it is a monorepo, and that monorepo must be an [Nx](https://nx.dev) workspace. Each Nx project is a folder with a `project.json`. The `name` in that file is the service name. KIKI lists those names as tags on the repository card. The selected tags are what the agents for that repository are told to work with. Agents confirm a project with `npx nx show projects`, run inside that repository, and read `root` from its `project.json`. `docs` stays a document folder and has no Nx projects.

Node.js needs to be on `PATH` so `npx nx` can run inside a code repository. Nx itself lives in that repository.

## Agents

| Group | Agents | Repository |
| --- | --- | --- |
| Requirement | Document Writer, UI Designer, Planner | Document. The planner also reads the three code repositories. |
| Backend | Senior Backend Developer, Backend Reviewer | Backend |
| Website | Senior Website Developer, Website Reviewer | Website |
| Mobile | Senior Mobile Developer, Mobile Reviewer | Mobile |

The writer turns a feature into `business_requirement.md` and `product_requirement.md`. The designer turns that product requirement into `design.html`. The planner turns the requirement folder into tickets. Each development pair then implements and reviews only inside its own repository.

KIKI copies each embedded role into the agent folder the first time that folder has no agent files. The Agents tab reads those files. A category is a folder, and each agent is `<category>/<id>.md`. Edit an agent in the app and press Save to write that file. Turning an agent off keeps the file and leaves that agent out of Start Feature.

## Start a feature

Feature work is started from KIKI and runs in Orca. **Start Feature** asks for a name, what the feature does, and which agents to turn on. Each selected agent is launched in Orca with the tool and model chosen for that agent.

KIKI creates one Orca project per repository that the selected agents use, groups those projects under the feature name, and opens a worktree for each project. The document agent writes the requirement into `docs/<feature-name>/`. Every other selected agent receives that folder path so it can read the requirement while it works in its own repository.

Orca must already be installed and running. The launch uses `orca agent launch` on the new worktree, in a terminal.

## Indexes

Agents look up code and notes through the index saved on each repository. That choice is made on the **Indexes** tab.

**Set up Obsidian** or **Set up CodeGraph** asks which repository to set up. KIKI writes the choice into `.kiki/repos/<kind>.yaml` and runs the tool in that repository's folder. A repository has one index. Choosing the other one replaces it. Obsidian is a vault searched with the [Obsidian CLI](https://obsidian.md/help/cli). CodeGraph is a code index created with `codegraph init <folder>`.

**Add Repository** chooses a repository and saves this index on it. The refresh icon beside the title checks every repository using that index and reports whether its `.obsidian` or `.codegraph` folder is still there. Opening the tab looks at each repository folder. When that folder already contains `.obsidian` or `.codegraph` and the repository has no index saved, KIKI writes that index into `.kiki/repos`. A repository that already has an index is left unchanged. If both folders are present, KIKI does not choose one. Opening the tab still runs the status command for each saved index. Obsidian CLI needs the [Obsidian](https://obsidian.md/) app, installer 1.12 or newer, and **Settings → General → Command line interface** turned on.

The launch prompt includes query instructions only for repositories whose file has an index. A repository with no index is left out of that section.

## Run

From this directory:

```sh
go build -o kiki .
./kiki
```

The window opens at `http://127.0.0.1:8787/`. Closing the window stops the app. `-addr` changes the listen address. `-no-open` serves that address without a window.

The build needs a C compiler because the window uses WebKit. On a Mac that is the Xcode command line tools.

These tools need to be on `PATH` for the matching action:

- `orca`, for Start Feature and any agent launch
- `obsidian`, for the document vault ([Obsidian CLI](https://obsidian.md/help/cli))
- `codegraph`, for the backend, website, and mobile indexes
- `git`, for Clone repository
- `node` and `npx`, so agents can run `npx nx` inside a code repository that is an Nx monorepo

## Settings

The gear next to the title opens Settings. Press Save to write the settings. Close discards anything that has not been saved.

- **Workspace** is the id stored as `workspace_id`. Use one short id: a letter, then letters, digits, `_`, or `-`.
- **Default AI Tool** is the tool a new agent card uses. The list is `cursor`, `claude`, `codex`, and `gemini`. Tools whose CLI is on `PATH` (`cursor-agent`, `claude`, `codex`, `gemini`) are listed first and can be chosen. The others stay in the list, grayed out, and cannot be chosen. Claude model names are the current alias versions from the Claude Code model config. The built-in list is Fable 5.2, Opus 5.5, Sonnet 5.5, and Haiku 5.5.
- **Generate text** is the AI Tool and model used by Generate in Edit agent. The sparkle icon at the bottom right of a prompt, repository note, or teamwork note opens the generate view in the Edit agent dialog. The model does not run yet. The CLI command is shown in a field, with Run on a row under the command. `{{workspace}}` stands in for the temporary folder and `{{prompt}}` stands in for the prompt. Run starts that tool's CLI (`cursor-agent`, `claude`, `codex`, or `gemini`). After Run, the command shows the real workspace path and `{{prompt}}`, then the model's thinking. The AI Tool and model sit at the top right of that field. After the model finishes, the bottom compares the current text and the rewrite in two columns, and Use becomes available. The back icon at the top left, and Cancel, return to the agent fields without changing the text. Use puts the rewrite in the field.
- **Agents** always live in `.kiki/agents` next to the app. Each category is a subfolder, and each agent is `<category>/<id>.md`. The file stores the id, name, description, whether the agent is on, AI tool, model, repos and what the agent does in each one, its Teamwork agents and how they coordinate, and the prompt. The launch prompt sent to Orca includes this agent's id, the work note for each repository, the query instructions for the index saved on each of those repositories, and each collaborator's id. On startup, KIKI creates that folder and, when it has no agent files yet, writes one file per embedded role. Each repository is written with the note that says what this agent does there, and each Teamwork agent is written with the note that says how this agent communicates with that agent id. A file that was saved as `<id>.md` at the top of the folder is moved into its category folder. Team folder also asks for your name. Save copies `.kiki/agents` to `<team>/<user>/agents`. If that agents folder is already there, KIKI asks whether to copy this app's agents into the team folder or copy the team folder into this app. After that save, Member agents stays on your user folder. Choose another member and press Save. KIKI asks you to confirm, then replaces `.kiki/agents` with that member's agents folder. Each agent card compares its file with the same file in your team folder. When the copies differ, Not Sync opens both files. Use asks you to confirm, and says which file replaces the other. Use on This app replaces the team copy. Use on Team folder replaces the copy in `.kiki/agents`. If a copied agent uses a repository this app does not have, KIKI asks you to create that repository or use one that already exists. Using an existing repository rewrites that repository id in the copied agent.

Edit an agent from its card and press Save. That writes the matching markdown file, including the AI tool and model.

## Files next to the binary

KIKI writes the files it creates in `.kiki` beside the executable, not in the current working directory. Repository folders stay beside the executable.

| File | Contents |
| --- | --- |
| `.kiki/kiki.yaml` | Workspace, default AI tool, repository folders, and selected services. Agents read this file. |
| `.kiki/repos/` | One file per repository: `backend.yaml`, `website.yaml`, `mobile.yaml`, and `document.yaml`. Each file stores that repository's folder, service names, and index (`obsidian` or `codegraph`). The launch prompt reads the index from these files. |
| `.kiki/agents/` | Agent files grouped by category folder, `<category>/<id>.md`. The Agents tab reads these files. Each file stores that agent's AI tool and model. |
| `.kiki/kiki.db` | Recent runs. Open a row for the detail, or delete and clear from the Agents tab. |

The repository folders are always `<app>/backend`, `<app>/website`, `<app>/mobile`, and `<app>/docs`. KIKI rewrites those paths in `.kiki/kiki.yaml` to match the folder it is running from.
