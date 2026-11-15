package server

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestRememberPresentIndexesSavesTheFolderThatExists(t *testing.T) {
	app := newTeamTestServer(t)
	if err := app.Store.EnsureLayout(app.Root); err != nil {
		t.Fatal(err)
	}
	backend := filepath.Join(app.Root, "backend")
	docs := filepath.Join(app.Root, "docs")
	website := filepath.Join(app.Root, "website")
	mobile := filepath.Join(app.Root, "mobile")
	for _, dir := range []string{
		filepath.Join(backend, ".codegraph"),
		filepath.Join(docs, ".obsidian"),
		filepath.Join(website, ".codegraph"),
		filepath.Join(website, ".obsidian"),
		filepath.Join(mobile, ".codegraph"),
	} {
		if err := os.Mkdir(dir, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	snap, err := app.load()
	if err != nil {
		t.Fatal(err)
	}
	if err := app.Store.SaveRepoIndex(snap, "mobile", "obsidian"); err != nil {
		t.Fatal(err)
	}
	snap, err = app.load()
	if err != nil {
		t.Fatal(err)
	}
	snap, err = app.rememberPresentIndexes(snap)
	if err != nil {
		t.Fatal(err)
	}
	if snap.Repo("backend").Index != "codegraph" {
		t.Fatalf("backend = %q", snap.Repo("backend").Index)
	}
	if snap.Repo("document").Index != "obsidian" {
		t.Fatalf("document = %q", snap.Repo("document").Index)
	}
	if snap.Repo("website").Index != "" {
		t.Fatalf("website = %q", snap.Repo("website").Index)
	}
	if snap.Repo("mobile").Index != "obsidian" {
		t.Fatalf("mobile = %q", snap.Repo("mobile").Index)
	}

	saved, err := app.load()
	if err != nil {
		t.Fatal(err)
	}
	if saved.Repo("backend").Index != "codegraph" || saved.Repo("document").Index != "obsidian" || saved.Repo("website").Index != "" || saved.Repo("mobile").Index != "obsidian" {
		t.Fatalf("saved backend=%q document=%q website=%q mobile=%q", saved.Repo("backend").Index, saved.Repo("document").Index, saved.Repo("website").Index, saved.Repo("mobile").Index)
	}
}

func TestUseIndexKeepsAnExistingFolder(t *testing.T) {
	app := newTeamTestServer(t)
	if err := app.Store.EnsureLayout(app.Root); err != nil {
		t.Fatal(err)
	}
	backend := filepath.Join(app.Root, "backend")
	if err := os.Mkdir(filepath.Join(backend, ".codegraph"), 0o755); err != nil {
		t.Fatal(err)
	}
	rec := httptest.NewRecorder()
	app.Handler().ServeHTTP(rec, httptest.NewRequest(http.MethodPut, "/api/indexes/backend", strings.NewReader(`{"index":"codegraph"}`)))
	if rec.Code != http.StatusOK {
		t.Fatalf("use status %d body %s", rec.Code, rec.Body.String())
	}
	var state map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &state); err != nil {
		t.Fatal(err)
	}
	if indexOf(t, state, "backend") != "codegraph" {
		t.Fatalf("backend index = %q", indexOf(t, state, "backend"))
	}
	present := presentOf(t, state, "backend")
	if len(present) != 1 || present[0] != "codegraph" {
		t.Fatalf("present = %#v", present)
	}

	missing := httptest.NewRecorder()
	app.Handler().ServeHTTP(missing, httptest.NewRequest(http.MethodPut, "/api/indexes/backend", strings.NewReader(`{"index":"obsidian"}`)))
	if missing.Code != http.StatusBadRequest {
		t.Fatalf("missing folder status %d body %s", missing.Code, missing.Body.String())
	}
	snap, err := app.load()
	if err != nil {
		t.Fatal(err)
	}
	if snap.Repo("backend").Index != "codegraph" {
		t.Fatalf("backend changed to %q", snap.Repo("backend").Index)
	}
}

func TestSetupKeepsAnExistingCodegraphFolder(t *testing.T) {
	app := newTeamTestServer(t)
	if err := app.Store.EnsureLayout(app.Root); err != nil {
		t.Fatal(err)
	}
	backend := filepath.Join(app.Root, "backend")
	if err := os.Mkdir(filepath.Join(backend, ".codegraph"), 0o755); err != nil {
		t.Fatal(err)
	}
	rec := httptest.NewRecorder()
	app.Handler().ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/api/indexes/codegraph/backend", nil))
	if rec.Code != http.StatusOK {
		t.Fatalf("status %d body %s", rec.Code, rec.Body.String())
	}
	var state map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &state); err != nil {
		t.Fatal(err)
	}
	if indexOf(t, state, "backend") != "codegraph" {
		t.Fatalf("backend index = %q", indexOf(t, state, "backend"))
	}
	jobs, _ := state["jobs"].([]any)
	if len(jobs) != 0 {
		t.Fatalf("setup started anyway: %s", rec.Body.String())
	}
}

func indexOf(t *testing.T, state map[string]any, kind string) string {
	t.Helper()
	for _, row := range state["repos"].([]any) {
		item := row.(map[string]any)
		if item["kind"] == kind {
			index, _ := item["index"].(string)
			return index
		}
	}
	t.Fatalf("repo %s missing", kind)
	return ""
}

func presentOf(t *testing.T, state map[string]any, kind string) []string {
	t.Helper()
	for _, row := range state["indexes"].([]any) {
		item := row.(map[string]any)
		if item["kind"] != kind {
			continue
		}
		raw, _ := item["present"].([]any)
		out := make([]string, 0, len(raw))
		for _, value := range raw {
			out = append(out, value.(string))
		}
		return out
	}
	t.Fatalf("index %s missing", kind)
	return nil
}
