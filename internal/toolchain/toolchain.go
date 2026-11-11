// Package toolchain builds the GitNexus and CodeGraph commands the agent
// documents tell the user to run.
package toolchain

import "path/filepath"

// Kinds are the repositories the console stores.
var Kinds = []string{"backend", "website", "mobile", "document"}

// CodeKinds are the repositories that get a CodeGraph index.
var CodeKinds = []string{"backend", "website", "mobile"}

// GitNexusAnalyze is `gitnexus analyze --index-only --name <workspace id>` at the repository root.
func GitNexusAnalyze(name string) []string {
	return []string{"gitnexus", "analyze", "--index-only", "--name", name}
}

// GitNexusStatus is `gitnexus status` at the repository root.
func GitNexusStatus() []string {
	return []string{"gitnexus", "status"}
}

// CodegraphInit is `codegraph init <folder>`.
func CodegraphInit(folder string) []string {
	return []string{"codegraph", "init", folder}
}

// CodegraphStatus is `codegraph status <folder>`.
func CodegraphStatus(folder string) []string {
	return []string{"codegraph", "status", folder}
}

// Folder is the absolute path of a repository under the app root.
// Document is the docs directory. The others use their kind as the directory name.
func Folder(root, kind string) string {
	name := kind
	if kind == "document" {
		name = "docs"
	}
	return filepath.Join(root, name)
}
