# KIKI

KIKI is a desktop console for a multi-agent software process. It defines who does which job, on which repository, and starts those agents together when a feature is ready to implement.

Each agent has one written role. The roles live in `agents/` and are embedded in the KIKI binary at build time. Agents do not share one checkout. Each works in its own repository, and they hand work to each other through the requirement folder and the indexes.

## Repositories

KIKI keeps four repositories as folders next to the binary. On startup it creates any of these that are missing.

| Folder | Kind | Who uses it |
| --- | --- | --- |
| `backend/` | backend | Backend developer and reviewer |
| `website/` | website | Website developer and reviewer |
| `mobile/` | mobile | Mobile developer and reviewer |
| `docs/` | document | Document writer, UI designer, and the planner |

`backend`, `website`, and `mobile` are the code repositories. `docs` is the document repository: requirements and screen designs live there, and it has no service list. An empty folder can be filled from the Repos tab with **Clone repository**.

Website, backend, and mobile services are read from each repo's `project.json` files and shown as tags. The selected tags are what the agents for that repository are told to work with.

## Agents

| Group | Agents | Repository |
| --- | --- | --- |
| Requirement | Document Writer, UI Designer, Planner | Document. The planner also reads the three code repositories. |
| Backend | Senior Backend Developer, Backend Reviewer | Backend |
| Website | Senior Website Developer, Website Reviewer | Website |
| Mobile | Senior Mobile Developer, Mobile Reviewer | Mobile |

The writer turns a feature into `business_requirement.md` and `product_requirement.md`. The designer turns that product requirement into `design.html`. The planner turns the requirement folder into tickets. Each development pair then implements and reviews only inside its own repository.

Edit a role by changing its markdown file under `agents/`, then rebuild. The running binary uses the embedded copy, not the file on disk.

## Start a feature

Feature work is started from KIKI and runs in Orca. **Start Feature** asks for a name, what the feature does, and which agents to turn on. Each selected agent is launched in Orca with the tool and model chosen for that agent.

KIKI creates one Orca project per repository that the selected agents use, groups those projects under the feature name, and opens a worktree for each project. The document agent writes the requirement into `docs/<feature-name>/`. Every other selected agent receives that folder path so it can read the requirement while it works in its own repository.

Orca must already be installed and running. The launch uses `orca agent launch` on the new worktree, in a terminal.

## Indexes

Agents look up code and repository facts through two indexes, both run from the **Indexes** tab.

**GitNexus** indexes the app folder under the workspace id. That index is how an agent finds information that lives in another repository, such as what a backend feature already does while writing a requirement. One workspace has one GitNexus index. Set the workspace in Settings before running it. The command is `gitnexus analyze --index-only --name <workspace>`.

**CodeGraph** indexes the code of `backend`, `website`, and `mobile`, one index per folder. Development agents use it to read and change that repository. The document repository is not indexed with CodeGraph. The command is `codegraph init <folder>`.

**Check** on an index card runs the matching status command and shows whether that index exists.

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
- `gitnexus`, for the workspace index
- `codegraph`, for the backend, website, and mobile indexes
- `git`, for Clone repository

## Settings

The gear next to the title opens Settings.

- **Workspace** is the id stored as `workspace_id`. It is also the GitNexus index name. Use one short id: a letter, then letters, digits, `_`, or `-`.
- **Default AI Tool** is the tool a new agent card uses. The choices are `cursor`, `claude`, `codex`, and `gemini`.

Each agent card can override the tool and the model. Those choices are saved separately from the workspace.

## Files next to the binary

KIKI reads and writes these beside the executable, not in the current working directory.

| File | Contents |
| --- | --- |
| `kiki.yaml` | Workspace, default AI tool, repository folders, and selected services. Agents read this file. |
| `models.yaml` | Per-agent AI tool and model. Copy this file beside KIKI on another machine to reuse the same choices. |
| `kiki.db` | Recent runs. Open a row for the detail, or delete and clear from the Agents tab. |

The repository folders are always `<app>/backend`, `<app>/website`, `<app>/mobile`, and `<app>/docs`. KIKI rewrites those paths in `kiki.yaml` to match the folder it is running from.
