package server

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"kiki/internal/jobs"
	"kiki/internal/orca"
	"kiki/internal/store"
)

func TestTeamSaveRejectsMissingFolder(t *testing.T) {
	app := newTeamTestServer(t)
	req := httptest.NewRequest(http.MethodPut, "/api/team", strings.NewReader(`{"path":"/missing/kiki-team"}`))
	rec := httptest.NewRecorder()
	app.handleTeamSave(rec, req)
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
}

func TestTeamMembersListsAnUnsavedFolder(t *testing.T) {
	app := newTeamTestServer(t)
	team := filepath.Join(t.TempDir(), "team")
	if err := os.MkdirAll(filepath.Join(team, "alice"), 0o755); err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodGet, "/api/team/members?path="+url.QueryEscape(team), nil)
	rec := httptest.NewRecorder()
	app.handleTeamMembers(rec, req)
	if rec.Code != http.StatusOK || !strings.Contains(rec.Body.String(), `"alice"`) {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	cfg, err := app.Store.TeamFolder()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Path != "" || cfg.Member != "" {
		t.Fatalf("folder was saved: %+v", cfg)
	}
}

func TestTeamBrowseReturnsChosenPath(t *testing.T) {
	app := newTeamTestServer(t)
	dir := t.TempDir()
	previous := chooseFolder
	chooseFolder = func() (string, error) { return dir + "/", nil }
	t.Cleanup(func() { chooseFolder = previous })
	req := httptest.NewRequest(http.MethodPost, "/api/team/browse", nil)
	rec := httptest.NewRecorder()
	app.handleTeamBrowse(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	var payload struct {
		Path string `json:"path"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &payload); err != nil {
		t.Fatal(err)
	}
	if payload.Path != dir {
		t.Fatalf("path = %q", payload.Path)
	}
}

func TestTeamMemberReplacesAgents(t *testing.T) {
	const shared = "---\nid: requirement_document\nname: From Team\ndescription: Shared copy.\nenabled: true\nai_tool: cursor\nmodel: auto\nrepos:\n  - document\n---\n\nHello from the team folder.\n"
	const local = "---\nid: backend_runner\nname: Local Runner\ndescription: Local copy.\nenabled: true\nai_tool: cursor\nmodel: auto\nrepos:\n  - backend\n---\n\nLocal.\n"
	app := newTeamTestServer(t)
	team := filepath.Join(t.TempDir(), "team")
	member := filepath.Join(team, "alice", "agents", "requirement")
	if err := os.MkdirAll(member, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(member, "requirement_document.md"), []byte(shared), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(team, "bob"), 0o755); err != nil {
		t.Fatal(err)
	}
	outside := filepath.Join(filepath.Dir(team), "outside")
	if err := os.MkdirAll(outside, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(outside, "secret.md"), []byte("secret"), 0o644); err != nil {
		t.Fatal(err)
	}
	agents := filepath.Join(app.Root, ".kiki", "agents", "backend")
	if err := os.MkdirAll(agents, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(agents, "backend_runner.md"), []byte(local), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(app.Root, ".kiki", "agents", "notes.txt"), []byte("notes"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := app.Store.SaveTeamFolder(store.TeamFolder{Path: team, Member: "bob"}); err != nil {
		t.Fatal(err)
	}

	req := httptest.NewRequest(http.MethodPost, "/api/team/member", strings.NewReader(`{"name":"../outside"}`))
	rec := httptest.NewRecorder()
	app.handleTeamMember(rec, req)
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("escape status = %d body = %s", rec.Code, rec.Body.String())
	}
	if _, err := os.Stat(filepath.Join(app.Root, ".kiki", "agents", "backend", "backend_runner.md")); err != nil {
		t.Fatal(err)
	}

	req = httptest.NewRequest(http.MethodGet, "/api/team/members", nil)
	rec = httptest.NewRecorder()
	app.handleTeamMembers(rec, req)
	if rec.Code != http.StatusOK || !strings.Contains(rec.Body.String(), `"alice"`) || strings.Contains(rec.Body.String(), "outside") {
		t.Fatalf("members status = %d body = %s", rec.Code, rec.Body.String())
	}

	req = httptest.NewRequest(http.MethodPost, "/api/team/member", strings.NewReader(`{"name":"alice"}`))
	rec = httptest.NewRecorder()
	app.handleTeamMember(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	var payload struct {
		Agents []struct {
			ID    string `json:"id"`
			Title string `json:"title"`
		} `json:"agents"`
		TeamFolder struct {
			Path   string `json:"path"`
			Member string `json:"member"`
		} `json:"teamFolder"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &payload); err != nil {
		t.Fatal(err)
	}
	if payload.TeamFolder.Member != "alice" || payload.TeamFolder.Path != team {
		t.Fatalf("team = %+v", payload.TeamFolder)
	}
	if len(payload.Agents) != 1 || payload.Agents[0].ID != "requirement_document" || payload.Agents[0].Title != "From Team" {
		t.Fatalf("agents = %+v", payload.Agents)
	}
	if _, err := os.Stat(filepath.Join(app.Root, ".kiki", "agents", "backend", "backend_runner.md")); !os.IsNotExist(err) {
		t.Fatal("local agent file was kept")
	}
	if _, err := os.Stat(filepath.Join(app.Root, ".kiki", "agents", "notes.txt")); !os.IsNotExist(err) {
		t.Fatal("local notes file was kept")
	}
	got, err := os.ReadFile(filepath.Join(app.Root, ".kiki", "agents", "requirement", "requirement_document.md"))
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != shared {
		t.Fatalf("copied file = %q", got)
	}

	other := filepath.Join(t.TempDir(), "other-team")
	if err := os.MkdirAll(other, 0o755); err != nil {
		t.Fatal(err)
	}
	req = httptest.NewRequest(http.MethodPut, "/api/team", strings.NewReader(`{"path":"`+other+`","user":"cara"}`))
	rec = httptest.NewRecorder()
	app.handleTeamSave(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("save status = %d body = %s", rec.Code, rec.Body.String())
	}
	cfg, err := app.Store.TeamFolder()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Path != other || cfg.User != "cara" || cfg.Member != "cara" {
		t.Fatalf("changed folder = %+v", cfg)
	}
}

func TestTeamSaveCopiesAgentsUnderTheUser(t *testing.T) {
	const local = "---\nid: backend_runner\nname: Local Runner\ndescription: Local copy.\nenabled: true\nai_tool: cursor\nmodel: auto\nrepos:\n  - backend\n---\n\nLocal.\n"
	app := newTeamTestServer(t)
	agents := filepath.Join(app.Root, ".kiki", "agents", "backend")
	if err := os.MkdirAll(agents, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(agents, "backend_runner.md"), []byte(local), 0o644); err != nil {
		t.Fatal(err)
	}
	team := filepath.Join(t.TempDir(), "team")
	if err := os.MkdirAll(team, 0o755); err != nil {
		t.Fatal(err)
	}
	body := `{"path":"` + team + `","user":"alice"}`
	req := httptest.NewRequest(http.MethodPut, "/api/team", strings.NewReader(body))
	rec := httptest.NewRecorder()
	app.handleTeamSave(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	got, err := os.ReadFile(filepath.Join(team, "alice", "agents", "backend", "backend_runner.md"))
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != local {
		t.Fatalf("copied = %q", got)
	}
	cfg, err := app.Store.TeamFolder()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Path != team || cfg.User != "alice" || cfg.Member != "alice" {
		t.Fatalf("team = %+v", cfg)
	}
}

func TestTeamSaveAsksWhenTheUserAlreadyHasAgents(t *testing.T) {
	app := newTeamTestServer(t)
	if err := os.MkdirAll(filepath.Join(app.Root, ".kiki", "agents", "backend"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(app.Root, ".kiki", "agents", "backend", "backend_runner.md"), []byte(localAgentFile), 0o644); err != nil {
		t.Fatal(err)
	}
	team := filepath.Join(t.TempDir(), "team")
	shared := filepath.Join(team, "alice", "agents", "requirement")
	if err := os.MkdirAll(shared, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(shared, "requirement_document.md"), []byte("team\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodPut, "/api/team", strings.NewReader(`{"path":"`+team+`","user":"alice"}`))
	rec := httptest.NewRecorder()
	app.handleTeamSave(rec, req)
	if rec.Code != http.StatusConflict || !strings.Contains(rec.Body.String(), `"conflict":"agents"`) {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	cfg, err := app.Store.TeamFolder()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Path != "" || cfg.User != "" {
		t.Fatalf("team was saved: %+v", cfg)
	}
	if _, err := os.Stat(filepath.Join(team, "alice", "agents", "backend", "backend_runner.md")); !os.IsNotExist(err) {
		t.Fatal("local agents were copied before a choice")
	}

	req = httptest.NewRequest(http.MethodPut, "/api/team", strings.NewReader(`{"path":"`+team+`","user":"alice","direction":"pull"}`))
	rec = httptest.NewRecorder()
	app.handleTeamSave(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("pull status = %d body = %s", rec.Code, rec.Body.String())
	}
	if _, err := os.Stat(filepath.Join(app.Root, ".kiki", "agents", "requirement", "requirement_document.md")); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(app.Root, ".kiki", "agents", "backend", "backend_runner.md")); !os.IsNotExist(err) {
		t.Fatal("local agent file was kept")
	}
	cfg, err = app.Store.TeamFolder()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.User != "alice" || cfg.Member != "alice" {
		t.Fatalf("team = %+v", cfg)
	}
}

func TestTeamMemberResolvesAnUnknownRepository(t *testing.T) {
	const shared = "---\nid: requirement_document\nname: From Team\ndescription: Shared copy.\nenabled: true\nai_tool: cursor\nmodel: auto\nrepos:\n  - id: payments\n    description: Take payments.\n---\n\nHello from the team folder.\n"
	app := newTeamTestServer(t)
	team := filepath.Join(t.TempDir(), "team")
	member := filepath.Join(team, "bob", "agents", "requirement")
	if err := os.MkdirAll(member, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(member, "requirement_document.md"), []byte(shared), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(app.Root, ".kiki", "agents"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(app.Root, ".kiki", "agents", "keep.md"), []byte("keep"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := app.Store.SaveTeamFolder(store.TeamFolder{Path: team, User: "alice", Member: "alice"}); err != nil {
		t.Fatal(err)
	}

	req := httptest.NewRequest(http.MethodPost, "/api/team/member", strings.NewReader(`{"name":"bob"}`))
	rec := httptest.NewRecorder()
	app.handleTeamMember(rec, req)
	if rec.Code != http.StatusConflict || !strings.Contains(rec.Body.String(), `"payments"`) {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	if _, err := os.Stat(filepath.Join(app.Root, ".kiki", "agents", "keep.md")); err != nil {
		t.Fatal(err)
	}

	req = httptest.NewRequest(http.MethodPost, "/api/team/member", strings.NewReader(`{"name":"bob","repos":{"payments":{"action":"use","id":"backend"}}}`))
	rec = httptest.NewRecorder()
	app.handleTeamMember(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("use status = %d body = %s", rec.Code, rec.Body.String())
	}
	got, err := os.ReadFile(filepath.Join(app.Root, ".kiki", "agents", "requirement", "requirement_document.md"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(got), "id: backend") || strings.Contains(string(got), "id: payments") {
		t.Fatalf("rewritten = %s", got)
	}
	if !strings.Contains(string(got), "Take payments.") {
		t.Fatalf("description was dropped:\n%s", got)
	}

	if err := os.WriteFile(filepath.Join(member, "requirement_document.md"), []byte(shared), 0o644); err != nil {
		t.Fatal(err)
	}
	req = httptest.NewRequest(http.MethodPost, "/api/team/member", strings.NewReader(`{"name":"bob","repos":{"payments":{"action":"create"}}}`))
	rec = httptest.NewRecorder()
	app.handleTeamMember(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("create status = %d body = %s", rec.Code, rec.Body.String())
	}
	got, err = os.ReadFile(filepath.Join(app.Root, ".kiki", "agents", "requirement", "requirement_document.md"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(got), "payments") {
		t.Fatalf("created copy = %s", got)
	}
	if _, err := os.Stat(filepath.Join(app.Root, "payments")); err != nil {
		t.Fatal(err)
	}
	repoFile, err := os.ReadFile(filepath.Join(filepath.Dir(app.Store.File()), "repos", "payments.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(repoFile), "id: payments") {
		t.Fatalf("repo file = %s", repoFile)
	}
	snap, err := app.load()
	if err != nil {
		t.Fatal(err)
	}
	if snap.Repo("payments").Folder != filepath.Join(app.Root, "payments") {
		t.Fatalf("payments = %+v", snap.Repo("payments"))
	}
}

const localAgentFile = "---\nid: backend_runner\nname: Local Runner\ndescription: Local copy.\nenabled: true\nai_tool: cursor\nmodel: auto\nrepos:\n  - backend\n---\n\nLocal.\n"

func TestAgentTeamDiffChoosesACopy(t *testing.T) {
	app := newTeamTestServer(t)
	localDir := filepath.Join(app.Root, ".kiki", "agents", "backend")
	if err := os.MkdirAll(localDir, 0o755); err != nil {
		t.Fatal(err)
	}
	localPath := filepath.Join(localDir, "backend_runner.md")
	if err := os.WriteFile(localPath, []byte(localAgentFile), 0o644); err != nil {
		t.Fatal(err)
	}
	team := t.TempDir()
	teamFile := filepath.Join(team, "alice", "agents", "backend", "backend_runner.md")
	if err := os.MkdirAll(filepath.Dir(teamFile), 0o755); err != nil {
		t.Fatal(err)
	}
	teamBody := strings.Replace(localAgentFile, "Local.", "From the team.", 1)
	if err := os.WriteFile(teamFile, []byte(teamBody), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := app.Store.SaveTeamFolder(store.TeamFolder{Path: team, User: "alice"}); err != nil {
		t.Fatal(err)
	}
	state, err := app.snapshot()
	if err != nil {
		t.Fatal(err)
	}
	agents, _ := state["agents"].([]map[string]any)
	var drift bool
	for _, agent := range agents {
		if agent["id"] == "backend_runner" {
			drift, _ = agent["teamDrift"].(bool)
		}
	}
	if !drift {
		t.Fatal("different copies were not marked")
	}
	rec := httptest.NewRecorder()
	app.Handler().ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/api/agents/backend_runner/team-diff", nil))
	if rec.Code != http.StatusOK || !strings.Contains(rec.Body.String(), "From the team.") || !strings.Contains(rec.Body.String(), `"kind":"del"`) {
		t.Fatalf("diff status = %d body = %s", rec.Code, rec.Body.String())
	}
	rec = httptest.NewRecorder()
	app.Handler().ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/api/agents/backend_runner/team-diff", strings.NewReader(`{"keep":"local"}`)))
	if rec.Code != http.StatusOK {
		t.Fatalf("keep local status = %d body = %s", rec.Code, rec.Body.String())
	}
	got, err := os.ReadFile(teamFile)
	if err != nil || string(got) != localAgentFile {
		t.Fatalf("team file = %q err = %v", got, err)
	}
	if err := os.WriteFile(teamFile, []byte(teamBody), 0o644); err != nil {
		t.Fatal(err)
	}
	rec = httptest.NewRecorder()
	app.Handler().ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/api/agents/backend_runner/team-diff", strings.NewReader(`{"keep":"team"}`)))
	if rec.Code != http.StatusOK {
		t.Fatalf("keep team status = %d body = %s", rec.Code, rec.Body.String())
	}
	got, err = os.ReadFile(localPath)
	if err != nil || string(got) != teamBody {
		t.Fatalf("local file = %q err = %v", got, err)
	}
}

func newTeamTestServer(t *testing.T) *Server {
	t.Helper()
	root := t.TempDir()
	db, err := store.Open(filepath.Join(root, "kiki.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = db.Close() })
	return &Server{
		Root:  root,
		Store: db,
		Orca:  &orca.Client{Dir: root},
		Jobs:  jobs.New(),
	}
}
