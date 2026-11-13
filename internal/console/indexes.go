package console

import (
	"fmt"
	"path/filepath"
	"strings"

	"kiki/internal/agentdef"
)

func writeIndexes(b *strings.Builder, repos []agentdef.Repo, snap Snapshot) {
	var blocks []string
	for _, item := range repos {
		kind := strings.TrimSpace(item.Kind)
		repo := snap.Repo(kind)
		switch repo.Index {
		case "obsidian":
			blocks = append(blocks, obsidianIndex(kind, repo, snap.Root))
		case "codegraph":
			blocks = append(blocks, codegraphIndex(kind, repo, snap.Root))
		}
	}
	if len(blocks) == 0 {
		return
	}
	b.WriteString("Repository indexes. For each repository this agent uses, read that repository's AGENTS.md, then use the index below. The index is the one saved for that repository. Do not set an index up yourself.\n\n")
	for _, block := range blocks {
		b.WriteString(block)
		if !strings.HasSuffix(block, "\n") {
			b.WriteString("\n")
		}
		b.WriteString("\n")
	}
}

func folderPath(folder string) string {
	folder = strings.TrimSpace(folder)
	if folder == "" {
		return "(not set)"
	}
	return folder
}

func indexFileLine(root, kind string) string {
	root = strings.TrimSpace(root)
	if root == "" {
		return ""
	}
	return fmt.Sprintf("Index file: %s\n\n", filepath.Join(root, ".kiki", "repos", kind+".yaml"))
}

func obsidianIndex(kind string, repo Repo, root string) string {
	path := folderPath(repo.Folder)
	read := "The folder is not set. Do not invent a path. Read this repository's AGENTS.md once the folder is set."
	if path != "(not set)" {
		read = fmt.Sprintf("Read %s/AGENTS.md before you search notes in this repository.", path)
	}
	return fmt.Sprintf(`### %s — Obsidian

Folder: %s

%s%s Follow that file for this repository. If it still tells you to use GitNexus, ignore that and follow this index.

The vault name is the last segment of the folder path, not the absolute path. Obsidian is set up when %s/.obsidian exists and this command, run in the folder, prints the folder:

`+"```"+`
obsidian vault=<name> vault info=path
`+"```"+`

vault=<name> is the first parameter. The vault is open in Obsidian. The CLI is the Obsidian CLI: Obsidian 1.12 or newer, with Settings → General → Command line interface enabled. The app must be running. The first command launches it if it is not.

Search and read. Do not list the folder. Use a path the task already names. In a document repository that is path=ui-system-design for the UI system, or the area folder for requirements.

`+"```"+`
obsidian vault=<name> search query="<words>" limit=20
obsidian vault=<name> search:context query="<words>" path="<folder>" limit=20
obsidian vault=<name> read path="<path from the vault root>"
obsidian vault=<name> backlinks path="<path>"
obsidian vault=<name> links path="<path>"
`+"```"+`

Open a file only when search, backlinks, or links point to it, or when the task already names the path.

If it is not set up, do not create the vault. Ask the user once: Read files (open only a path you already know, and say this index was not used) or Wait for setup (stop until the user sets up Obsidian on this folder in KIKI). Do not choose for the user. If the search returns nothing useful, ask the user. Do not walk the folder unless the user chose Read files.
`, kind, path, indexFileLine(root, kind), read, path)
}

func codegraphIndex(kind string, repo Repo, root string) string {
	path := folderPath(repo.Folder)
	services := "(none)"
	if len(repo.Services) > 0 {
		services = strings.Join(repo.Services, ", ")
	}
	read := "The folder is not set. Do not invent a path. Read this repository's AGENTS.md, and each service AGENTS.md, once the folder is set."
	if path != "(not set)" {
		read = fmt.Sprintf("Before you read code, read %s/AGENTS.md. For each service this work touches, also read %s/<project root>/AGENTS.md.", path, path)
	}
	return fmt.Sprintf(`### %s — CodeGraph

Folder: %s

%sServices: %s

%s A service is an Nx project from the services list. Find its root in project.json (`+"`npx nx show projects`"+`, run in the folder). If the project root is ".", that file is the folder file; read it once. The folder file covers the repository. The service file covers the files in that service. When they conflict inside a service, follow the service file. If AGENTS.md is missing, ask the user once and stop. Do not invent the structure or the rules.

CodeGraph is set up when %s/.codegraph exists and `+"`codegraph status %s`"+` runs without a "not initialized" error. Do not run `+"`codegraph init`"+` yourself, and do not install CodeGraph. If it is not set up, ask the user once: Read files (search the exact symbol or path and read only the files you need, and say this index was not used) or Wait for setup (stop until the user sets up CodeGraph on this folder in KIKI). Do not choose for the user.

Query this index before grep, find, or reading the repository.

`+"```"+`
codegraph explore -p %s "<question>"
codegraph callers -p %s <symbol>
codegraph callees -p %s <symbol>
codegraph impact -p %s <symbol>
codegraph affected -p %s <files>
codegraph sync %s
`+"```"+`

MCP: codegraph_explore with projectPath set to the folder, and the question in query. Run codegraph sync when codegraph status shows pending changes.

Open a file only when the result marks it stale, when the graph does not cover it (schemas, generated code, fixtures, config, assets), or when AGENTS.md tells you to. Do not read the whole repository. .codegraph/ is gitignored; leave it uncommitted.
`, kind, path, indexFileLine(root, kind), services, read, path, path, path, path, path, path, path, path)
}
