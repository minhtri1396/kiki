package server

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"kiki/internal/console"
	"kiki/internal/toolchain"
)

func (s *Server) handleClone(w http.ResponseWriter, r *http.Request) {
	kind := r.PathValue("kind")
	if !toolchain.RepoID(kind) {
		writeErr(w, http.StatusNotFound, errors.New("unknown repo"))
		return
	}
	var body struct {
		URL    string `json:"url"`
		Sparse string `json:"sparse"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	remote := strings.TrimSpace(body.URL)
	if !validGitURL(remote) {
		writeErr(w, http.StatusBadRequest, errors.New("Git URL is not valid"))
		return
	}
	sparse, err := console.SparsePath(body.Sparse)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	folder := toolchain.Folder(s.Root, kind)
	if err := os.MkdirAll(folder, 0o755); err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	empty, err := folderEmpty(folder)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if !empty {
		writeErr(w, http.StatusBadRequest, errors.New("folder is not empty"))
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), 5*time.Minute)
	defer cancel()
	if err := gitClone(ctx, remote, folder, sparse); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	snap, err := s.load()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if err := s.Store.SaveRepoSparse(snap, kind, sparse); err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	s.handleState(w, r)
}

func validGitURL(raw string) bool {
	if raw == "" || strings.ContainsAny(raw, " \t\r\n") || strings.HasPrefix(raw, "-") {
		return false
	}
	if strings.HasPrefix(raw, "git@") {
		host, path, ok := strings.Cut(strings.TrimPrefix(raw, "git@"), ":")
		return ok && host != "" && !strings.Contains(host, "/") && path != ""
	}
	parsed, err := url.Parse(raw)
	if err != nil || parsed.Host == "" && parsed.Scheme != "file" {
		return false
	}
	switch parsed.Scheme {
	case "https", "http", "ssh", "git":
		return parsed.Host != ""
	case "file":
		return parsed.Path != ""
	default:
		return false
	}
}

// folderEmpty reports whether path has no repository content.
// A missing folder is empty. .DS_Store and a CodeGraph or Obsidian setup do not count.
func folderEmpty(path string) (bool, error) {
	entries, err := os.ReadDir(path)
	if err != nil {
		if os.IsNotExist(err) {
			return true, nil
		}
		return false, err
	}
	for _, entry := range entries {
		if setupEntry(entry.Name()) {
			continue
		}
		return false, nil
	}
	return true, nil
}

func setupEntry(name string) bool {
	switch name {
	case ".DS_Store", ".codegraph", ".obsidian":
		return true
	default:
		return false
	}
}

func gitClone(ctx context.Context, remote, folder, sparse string) error {
	if err := os.MkdirAll(folder, 0o755); err != nil {
		return err
	}
	tmp, err := os.MkdirTemp(filepath.Dir(folder), ".kiki-clone-")
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmp)
	if err := runGitClone(ctx, remote, tmp, sparse); err != nil {
		return err
	}
	if sparse != "" {
		if err := sparseCheckout(ctx, tmp, sparse); err != nil {
			return err
		}
	}
	return takeClone(tmp, folder)
}

func runGitClone(ctx context.Context, remote, folder, sparse string) error {
	if sparse == "" {
		return execGit(ctx, "clone", "--", remote, folder)
	}
	err := execGit(ctx, "clone", "--filter=blob:none", "--sparse", "--", remote, folder)
	if err == nil || ctx.Err() != nil || !filterRejected(err) {
		return err
	}
	if err := os.RemoveAll(folder); err != nil {
		return err
	}
	if err := os.MkdirAll(folder, 0o755); err != nil {
		return err
	}
	return execGit(ctx, "clone", "--sparse", "--", remote, folder)
}

func sparseCheckout(ctx context.Context, folder, sparse string) error {
	if err := execGit(ctx, "-C", folder, "sparse-checkout", "set", "--cone", "--", sparse); err != nil {
		return err
	}
	info, err := os.Stat(filepath.Join(folder, filepath.FromSlash(sparse)))
	if err != nil || !info.IsDir() {
		return fmt.Errorf("folder %s is not in this repository", sparse)
	}
	return nil
}

func filterRejected(err error) bool {
	return strings.Contains(strings.ToLower(err.Error()), "filter")
}

func execGit(ctx context.Context, args ...string) error {
	cmd := exec.CommandContext(ctx, "git", args...)
	cmd.Env = append(os.Environ(), "GIT_TERMINAL_PROMPT=0")
	out, err := cmd.CombinedOutput()
	if err == nil {
		return nil
	}
	name := gitCommandName(args)
	if ctx.Err() != nil {
		return fmt.Errorf("git %s timed out", name)
	}
	text := strings.TrimSpace(string(out))
	if text == "" {
		text = err.Error()
	}
	if len(text) > 2000 {
		text = text[:2000] + "…"
	}
	return fmt.Errorf("git %s failed: %s", name, text)
}

func gitCommandName(args []string) string {
	for _, arg := range args {
		if arg != "" && !strings.HasPrefix(arg, "-") {
			return arg
		}
	}
	return "command"
}

// takeClone moves a finished clone into folder and drops index setup that was there.
func takeClone(src, dest string) error {
	entries, err := os.ReadDir(dest)
	if err != nil {
		return err
	}
	for _, entry := range entries {
		if !setupEntry(entry.Name()) {
			continue
		}
		if err := os.RemoveAll(filepath.Join(dest, entry.Name())); err != nil {
			return err
		}
	}
	cloned, err := os.ReadDir(src)
	if err != nil {
		return err
	}
	for _, entry := range cloned {
		from := filepath.Join(src, entry.Name())
		to := filepath.Join(dest, entry.Name())
		if err := os.Rename(from, to); err != nil {
			return err
		}
	}
	return nil
}
