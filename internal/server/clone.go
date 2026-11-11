package server

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"strings"
	"time"

	"kiki/internal/toolchain"
)

func (s *Server) handleClone(w http.ResponseWriter, r *http.Request) {
	kind := r.PathValue("kind")
	if !knownRepo(kind) {
		writeErr(w, http.StatusNotFound, errors.New("unknown repo"))
		return
	}
	var body struct {
		URL string `json:"url"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	remote := strings.TrimSpace(body.URL)
	if !validGitURL(remote) {
		writeErr(w, http.StatusBadRequest, errors.New("Git URL is not valid"))
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
	if err := gitClone(ctx, remote, folder); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	s.handleState(w, r)
}

func knownRepo(kind string) bool {
	for _, item := range toolchain.Kinds {
		if item == kind {
			return true
		}
	}
	return false
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

func folderEmpty(path string) (bool, error) {
	entries, err := os.ReadDir(path)
	if err != nil {
		if os.IsNotExist(err) {
			return true, nil
		}
		return false, err
	}
	for _, entry := range entries {
		if entry.Name() == ".DS_Store" {
			continue
		}
		return false, nil
	}
	return true, nil
}

func gitClone(ctx context.Context, remote, folder string) error {
	cmd := exec.CommandContext(ctx, "git", "clone", "--", remote, folder)
	cmd.Env = append(os.Environ(), "GIT_TERMINAL_PROMPT=0")
	out, err := cmd.CombinedOutput()
	if err == nil {
		return nil
	}
	if ctx.Err() != nil {
		return fmt.Errorf("git clone timed out")
	}
	text := strings.TrimSpace(string(out))
	if text == "" {
		text = err.Error()
	}
	if len(text) > 2000 {
		text = text[:2000] + "…"
	}
	return fmt.Errorf("git clone failed: %s", text)
}
