// Package toolchain builds the Obsidian and CodeGraph commands the console runs
// and the paths of the app repositories.
package toolchain

import (
	"path/filepath"
	"regexp"
)

// Kinds are the repositories the console stores.
var Kinds = []string{"backend", "website", "mobile", "document"}

var repoIDPattern = regexp.MustCompile(`^[a-z][a-z0-9_]*$`)

// Known reports whether kind is one of the built-in app repositories.
func Known(kind string) bool {
	for _, item := range Kinds {
		if item == kind {
			return true
		}
	}
	return false
}

// RepoID reports whether kind can be stored as a repository id.
func RepoID(kind string) bool {
	return repoIDPattern.MatchString(kind)
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
