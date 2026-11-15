package server

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"kiki/internal/jobs"
	"kiki/internal/orca"
	"kiki/internal/store"
)

func TestTeamSaveRequiresAName(t *testing.T) {
	app := newTeamTestServer(t)
	team := filepath.Join(t.TempDir(), "team")
	if err := os.MkdirAll(team, 0o755); err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodPut, "/api/team", strings.NewReader(`{"path":"`+team+`","user":"  "}`))
	rec := httptest.NewRecorder()
	app.handleTeamSave(rec, req)
	if rec.Code != http.StatusBadRequest || !strings.Contains(rec.Body.String(), "Enter your name.") {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	cfg, err := app.Store.TeamFolder()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Path != "" || cfg.User != "" {
		t.Fatalf("folder was saved: %+v", cfg)
	}
}

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

func teamFlag(t *testing.T, app *Server, name string) bool {
	t.Helper()
	state, err := app.snapshot()
	if err != nil {
		t.Fatal(err)
	}
	team, ok := state["teamFolder"].(map[string]any)
	if !ok {
		t.Fatalf("team folder = %#v", state["teamFolder"])
	}
	value, ok := team[name].(bool)
	if !ok {
		t.Fatalf("%s = %#v", name, team[name])
	}
	return value
}

func teamStale(t *testing.T, app *Server) bool {
	t.Helper()
	return teamFlag(t, app, "stale")
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
	if err := os.MkdirAll(filepath.Join(team, "bob", "agents"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(team, "bob", "agents", "old.md"), []byte("old"), 0o644); err != nil {
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
	if err := app.Store.SaveTeamFolder(store.TeamFolder{Path: team, User: "bob", Member: "bob"}); err != nil {
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
			Path      string `json:"path"`
			Member    string `json:"member"`
			Stale     bool   `json:"stale"`
			UserStale bool   `json:"userStale"`
		} `json:"teamFolder"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &payload); err != nil {
		t.Fatal(err)
	}
	if payload.TeamFolder.Member != "alice" || payload.TeamFolder.Path != team || payload.TeamFolder.Stale || !payload.TeamFolder.UserStale {
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
	if _, err := os.Stat(filepath.Join(team, "bob", "agents", "old.md")); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(team, "bob", "agents", "requirement", "requirement_document.md")); !os.IsNotExist(err) {
		t.Fatal("user agents folder was overwritten")
	}
	source, err := os.ReadFile(filepath.Join(member, "requirement_document.md"))
	if err != nil {
		t.Fatal(err)
	}
	if string(source) != shared {
		t.Fatalf("source member = %q", source)
	}
	copied := filepath.Join(app.Root, ".kiki", "agents", "requirement", "requirement_document.md")
	if err := os.WriteFile(copied, []byte(shared+"\nlocal edit\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if !teamStale(t, app) {
		t.Fatal(".kiki/agents differing from the member did not show sync")
	}
	if err := os.WriteFile(copied, []byte(shared), 0o644); err != nil {
		t.Fatal(err)
	}
	if teamStale(t, app) {
		t.Fatal("matching agents were reported as different")
	}
	updated := shared + "\nUpdated by the member.\n"
	if err := os.WriteFile(filepath.Join(member, "requirement_document.md"), []byte(updated), 0o644); err != nil {
		t.Fatal(err)
	}
	if !teamStale(t, app) {
		t.Fatal("member update was not reported")
	}
	req = httptest.NewRequest(http.MethodPost, "/api/team/member", strings.NewReader(`{"name":"alice"}`))
	rec = httptest.NewRecorder()
	app.handleTeamMember(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("pull status = %d body = %s", rec.Code, rec.Body.String())
	}
	if teamStale(t, app) || !teamFlag(t, app, "userStale") {
		t.Fatal("pull changed the user folder or left the member different")
	}
	if _, err := os.Stat(filepath.Join(team, "bob", "agents", "old.md")); err != nil {
		t.Fatal(err)
	}
	req = httptest.NewRequest(http.MethodPost, "/api/team/member", strings.NewReader(`{"name":"bob","direction":"push"}`))
	rec = httptest.NewRecorder()
	app.handleTeamMember(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("push status = %d body = %s", rec.Code, rec.Body.String())
	}
	if teamStale(t, app) || teamFlag(t, app, "userStale") {
		t.Fatal("synced member is still different")
	}
	for _, path := range []string{
		copied,
		filepath.Join(team, "bob", "agents", "requirement", "requirement_document.md"),
	} {
		body, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		if string(body) != updated {
			t.Fatalf("%s = %q", path, body)
		}
	}
	if _, err := os.Stat(filepath.Join(team, "bob", "agents", "old.md")); !os.IsNotExist(err) {
		t.Fatal("user agent file was kept")
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

func TestTeamMemberRequiresAUser(t *testing.T) {
	app := newTeamTestServer(t)
	team := filepath.Join(t.TempDir(), "team")
	member := filepath.Join(team, "alice", "agents")
	if err := os.MkdirAll(member, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(member, "note.md"), []byte("shared"), 0o644); err != nil {
		t.Fatal(err)
	}
	local := filepath.Join(app.Root, ".kiki", "agents")
	if err := os.MkdirAll(local, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(local, "keep.md"), []byte("keep"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := app.Store.SaveTeamFolder(store.TeamFolder{Path: team}); err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodPost, "/api/team/member", strings.NewReader(`{"name":"alice"}`))
	rec := httptest.NewRecorder()
	app.handleTeamMember(rec, req)
	if rec.Code != http.StatusBadRequest || !strings.Contains(rec.Body.String(), "Settings") {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	if _, err := os.Stat(filepath.Join(local, "keep.md")); err != nil {
		t.Fatal(err)
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

func TestTeamMembersReportsFoldersThatDiffer(t *testing.T) {
	app := newTeamTestServer(t)
	agents := filepath.Join(app.Root, ".kiki", "agents")
	if err := os.MkdirAll(agents, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(agents, "local.md"), []byte("local\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	team := filepath.Join(t.TempDir(), "team")
	if err := os.MkdirAll(filepath.Join(team, "bob", "agents"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(team, "bob", "agents", "bob.md"), []byte("bob\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(team, "cara"), 0o755); err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodPut, "/api/team", strings.NewReader(`{"path":"`+team+`","user":"alice"}`))
	rec := httptest.NewRecorder()
	app.handleTeamSave(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("save status = %d body = %s", rec.Code, rec.Body.String())
	}

	req = httptest.NewRequest(http.MethodGet, "/api/team/members", nil)
	rec = httptest.NewRecorder()
	app.handleTeamMembers(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	var payload struct {
		Members []string `json:"members"`
		Differs []string `json:"differs"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &payload); err != nil {
		t.Fatal(err)
	}
	if !slices.Contains(payload.Differs, "bob") || !slices.Contains(payload.Differs, "cara") || slices.Contains(payload.Differs, "alice") {
		t.Fatalf("differs = %#v", payload.Differs)
	}
}

func TestTeamCompareListsAppUserAndMember(t *testing.T) {
	app := newTeamTestServer(t)
	localDir := filepath.Join(app.Root, ".kiki", "agents", "backend")
	if err := os.MkdirAll(localDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(localDir, "backend_runner.md"), []byte(localAgentFile), 0o644); err != nil {
		t.Fatal(err)
	}
	team := filepath.Join(t.TempDir(), "team")
	if err := os.MkdirAll(filepath.Join(team, "bob", "agents", "backend"), 0o755); err != nil {
		t.Fatal(err)
	}
	bob := strings.Replace(localAgentFile, "Local Runner", "Bob Runner", 1)
	if err := os.WriteFile(filepath.Join(team, "bob", "agents", "backend", "backend_runner.md"), []byte(bob), 0o644); err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodPut, "/api/team", strings.NewReader(`{"path":"`+team+`","user":"alice"}`))
	rec := httptest.NewRecorder()
	app.handleTeamSave(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("save status = %d body = %s", rec.Code, rec.Body.String())
	}

	req = httptest.NewRequest(http.MethodGet, "/api/team/compare?member=bob", nil)
	rec = httptest.NewRecorder()
	app.handleTeamCompare(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	var payload struct {
		Members []string `json:"members"`
		App     []struct {
			Name string `json:"name"`
			Hash string `json:"hash"`
		} `json:"app"`
		User struct {
			Name    string `json:"name"`
			Differs bool   `json:"differs"`
			Agents  []struct {
				Name string `json:"name"`
				Hash string `json:"hash"`
			} `json:"agents"`
		} `json:"user"`
		Member struct {
			Name    string `json:"name"`
			Differs bool   `json:"differs"`
			Agents  []struct {
				Name string `json:"name"`
				Hash string `json:"hash"`
			} `json:"agents"`
		} `json:"member"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &payload); err != nil {
		t.Fatal(err)
	}
	if !slices.Contains(payload.Members, "alice") || !slices.Contains(payload.Members, "bob") {
		t.Fatalf("members = %#v", payload.Members)
	}
	if payload.User.Name != "alice" || payload.User.Differs || len(payload.User.Agents) != 1 || payload.User.Agents[0].Name != "Local Runner" {
		t.Fatalf("user = %+v", payload.User)
	}
	if payload.Member.Name != "bob" || !payload.Member.Differs || len(payload.Member.Agents) != 1 || payload.Member.Agents[0].Name != "Bob Runner" {
		t.Fatalf("member = %+v", payload.Member)
	}
	if len(payload.App) != 1 || payload.App[0].Name != "Local Runner" {
		t.Fatalf("app = %+v", payload.App)
	}
	if payload.App[0].Hash == "" || payload.User.Agents[0].Hash != payload.App[0].Hash || payload.Member.Agents[0].Hash == payload.App[0].Hash {
		t.Fatalf("hashes app=%q user=%q member=%q", payload.App[0].Hash, payload.User.Agents[0].Hash, payload.Member.Agents[0].Hash)
	}

	req = httptest.NewRequest(http.MethodGet, "/api/team/diff?folder=bob&id=backend_runner", nil)
	rec = httptest.NewRecorder()
	app.handleTeamDiff(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("diff status = %d body = %s", rec.Code, rec.Body.String())
	}
	var diff struct {
		Name string `json:"name"`
		Diff []struct {
			Kind string `json:"kind"`
			Text string `json:"text"`
		} `json:"diff"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &diff); err != nil {
		t.Fatal(err)
	}
	if diff.Name != "Local Runner" {
		t.Fatalf("diff name = %q", diff.Name)
	}
	var sawLocal, sawBob bool
	for _, line := range diff.Diff {
		if line.Kind == "del" && strings.Contains(line.Text, "Local Runner") {
			sawLocal = true
		}
		if line.Kind == "add" && strings.Contains(line.Text, "Bob Runner") {
			sawBob = true
		}
	}
	if !sawLocal || !sawBob {
		t.Fatalf("diff = %+v", diff.Diff)
	}
}

func TestTeamDiffShowsACategoryChange(t *testing.T) {
	app := newTeamTestServer(t)
	localDir := filepath.Join(app.Root, ".kiki", "agents", "backend")
	if err := os.MkdirAll(localDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(localDir, "backend_runner.md"), []byte(localAgentFile), 0o644); err != nil {
		t.Fatal(err)
	}
	team := filepath.Join(t.TempDir(), "team")
	moved := filepath.Join(team, "bob", "agents", "frontend")
	if err := os.MkdirAll(moved, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(moved, "backend_runner.md"), []byte(localAgentFile), 0o644); err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodPut, "/api/team", strings.NewReader(`{"path":"`+team+`","user":"alice"}`))
	rec := httptest.NewRecorder()
	app.handleTeamSave(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("save status = %d body = %s", rec.Code, rec.Body.String())
	}

	req = httptest.NewRequest(http.MethodGet, "/api/team/compare?member=bob", nil)
	rec = httptest.NewRecorder()
	app.handleTeamCompare(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("compare status = %d body = %s", rec.Code, rec.Body.String())
	}
	var compared struct {
		App []struct {
			Category string `json:"category"`
			Hash     string `json:"hash"`
		} `json:"app"`
		Member struct {
			Differs bool `json:"differs"`
			Agents  []struct {
				Category string `json:"category"`
				Hash     string `json:"hash"`
			} `json:"agents"`
		} `json:"member"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &compared); err != nil {
		t.Fatal(err)
	}
	if len(compared.App) != 1 || len(compared.Member.Agents) != 1 {
		t.Fatalf("compare = %+v", compared)
	}
	if compared.App[0].Category != "backend" || compared.Member.Agents[0].Category != "frontend" {
		t.Fatalf("categories = %q %q", compared.App[0].Category, compared.Member.Agents[0].Category)
	}
	if !compared.Member.Differs || compared.App[0].Hash == "" || compared.App[0].Hash != compared.Member.Agents[0].Hash {
		t.Fatalf("member = %+v app = %+v", compared.Member, compared.App)
	}

	req = httptest.NewRequest(http.MethodGet, "/api/team/diff?folder=bob&id=backend_runner", nil)
	rec = httptest.NewRecorder()
	app.handleTeamDiff(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("diff status = %d body = %s", rec.Code, rec.Body.String())
	}
	var diff struct {
		Diff []struct {
			Kind string `json:"kind"`
			Text string `json:"text"`
		} `json:"diff"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &diff); err != nil {
		t.Fatal(err)
	}
	var sawBackend, sawFrontend bool
	for _, line := range diff.Diff {
		if line.Kind == "del" && line.Text == "category: backend" {
			sawBackend = true
		}
		if line.Kind == "add" && line.Text == "category: frontend" {
			sawFrontend = true
		}
	}
	if !sawBackend || !sawFrontend {
		t.Fatalf("diff = %+v", diff.Diff)
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
	if rec.Code != http.StatusConflict || !strings.Contains(rec.Body.String(), `"conflict":"name"`) {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	cfg, err := app.Store.TeamFolder()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Path != "" || cfg.User != "" {
		t.Fatalf("folder was saved before a choice: %+v", cfg)
	}
	req = httptest.NewRequest(http.MethodPut, "/api/team", strings.NewReader(`{"path":"`+team+`","user":"alice","useExisting":true}`))
	rec = httptest.NewRecorder()
	app.handleTeamSave(rec, req)
	if rec.Code != http.StatusConflict || !strings.Contains(rec.Body.String(), `"conflict":"agents"`) {
		t.Fatalf("own folder status = %d body = %s", rec.Code, rec.Body.String())
	}
	cfg, err = app.Store.TeamFolder()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Path != "" || cfg.User != "" {
		t.Fatalf("team changed before a choice: %+v", cfg)
	}
	if _, err := os.Stat(filepath.Join(team, "alice", "agents", "backend", "backend_runner.md")); !os.IsNotExist(err) {
		t.Fatal("local agents were copied before a choice")
	}

	req = httptest.NewRequest(http.MethodPut, "/api/team", strings.NewReader(`{"path":"`+team+`","user":"alice","useExisting":true,"direction":"pull"}`))
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

	req = httptest.NewRequest(http.MethodPost, "/api/team/member", strings.NewReader(`{"name":"bob","direction":"pull","repos":{"payments":{"action":"use","id":"backend"}}}`))
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
	source, err := os.ReadFile(filepath.Join(member, "requirement_document.md"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(source), "id: payments") {
		t.Fatalf("source member was rewritten: %s", source)
	}

	if err := os.WriteFile(filepath.Join(member, "requirement_document.md"), []byte(shared), 0o644); err != nil {
		t.Fatal(err)
	}
	req = httptest.NewRequest(http.MethodPost, "/api/team/member", strings.NewReader(`{"name":"bob","direction":"pull","repos":{"payments":{"action":"create"}}}`))
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

func TestSaveUserAgents(t *testing.T) {
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
	if err := os.WriteFile(filepath.Join(team, "alice", "agents", "old.md"), []byte("old"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := app.Store.SaveTeamFolder(store.TeamFolder{Path: team, User: "alice", Member: "alice"}); err != nil {
		t.Fatal(err)
	}
	if !teamFlag(t, app, "userStale") {
		t.Fatal("different agents did not show save")
	}
	rec := httptest.NewRecorder()
	app.Handler().ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/api/team/agents", nil))
	if rec.Code != http.StatusOK {
		t.Fatalf("save status = %d body = %s", rec.Code, rec.Body.String())
	}
	got, err := os.ReadFile(teamFile)
	if err != nil || string(got) != localAgentFile {
		t.Fatalf("team file = %q err = %v", got, err)
	}
	if _, err := os.Stat(filepath.Join(team, "alice", "agents", "old.md")); !os.IsNotExist(err) {
		t.Fatal("old team file was kept")
	}
	kept, err := os.ReadFile(localPath)
	if err != nil || string(kept) != localAgentFile {
		t.Fatalf("local file = %q err = %v", kept, err)
	}
	if teamFlag(t, app, "userStale") {
		t.Fatal("saved agents were still different")
	}
	cfg, err := app.Store.TeamFolder()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Member != "alice" {
		t.Fatalf("member = %q", cfg.Member)
	}
}

func TestTeamMemberPushOverwritesTheSelectedFolder(t *testing.T) {
	app := newTeamTestServer(t)
	localDir := filepath.Join(app.Root, ".kiki", "agents", "backend")
	if err := os.MkdirAll(localDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(localDir, "backend_runner.md"), []byte(localAgentFile), 0o644); err != nil {
		t.Fatal(err)
	}
	team := t.TempDir()
	if err := os.MkdirAll(filepath.Join(team, "cara"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := app.Store.SaveTeamFolder(store.TeamFolder{Path: team, User: "alice", Member: "alice"}); err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodPost, "/api/team/member", strings.NewReader(`{"name":"cara","direction":"pull"}`))
	rec := httptest.NewRecorder()
	app.handleTeamMember(rec, req)
	if rec.Code != http.StatusBadRequest || !strings.Contains(rec.Body.String(), "no agents folder") {
		t.Fatalf("pull status = %d body = %s", rec.Code, rec.Body.String())
	}
	req = httptest.NewRequest(http.MethodPost, "/api/team/member", strings.NewReader(`{"name":"cara","direction":"sideways"}`))
	rec = httptest.NewRecorder()
	app.handleTeamMember(rec, req)
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("direction status = %d body = %s", rec.Code, rec.Body.String())
	}
	req = httptest.NewRequest(http.MethodPost, "/api/team/member", strings.NewReader(`{"name":"cara","direction":"push"}`))
	rec = httptest.NewRecorder()
	app.handleTeamMember(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("push status = %d body = %s", rec.Code, rec.Body.String())
	}
	got, err := os.ReadFile(filepath.Join(team, "cara", "agents", "backend", "backend_runner.md"))
	if err != nil || string(got) != localAgentFile {
		t.Fatalf("pushed = %q err = %v", got, err)
	}
	kept, err := os.ReadFile(filepath.Join(localDir, "backend_runner.md"))
	if err != nil || string(kept) != localAgentFile {
		t.Fatalf("local = %q err = %v", kept, err)
	}
	cfg, err := app.Store.TeamFolder()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Member != "cara" || cfg.User != "alice" {
		t.Fatalf("team = %+v", cfg)
	}
	if teamStale(t, app) {
		t.Fatal("pushed agents were still different")
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
		Root:       root,
		Store:      db,
		Orca:       &orca.Client{Dir: root},
		Jobs:       jobs.New(),
		RTKHooks:   func(string, bool) error { return nil },
		GuardHooks: func(string, bool) error { return nil },
	}
}
