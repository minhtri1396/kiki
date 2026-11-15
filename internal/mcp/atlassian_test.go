package mcp

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"gopkg.in/yaml.v3"
)

func TestApplyKeepsBlankToken(t *testing.T) {
	prev := Atlassian{
		Enabled:  true,
		Username: "a@b.co",
		APIToken: "keep-me",
		Jira:     Service{Enabled: true, URL: "https://old.atlassian.net"},
		Confluence: Service{
			Enabled: true, URL: "https://old.atlassian.net/wiki", Own: true, Username: "c@b.co", APIToken: "other",
		},
	}
	next := Apply(prev, Atlassian{
		Enabled:    true,
		Username:   "new@acme.co",
		Jira:       Service{Enabled: true, URL: "https://acme.atlassian.net/"},
		Confluence: Service{Enabled: true, URL: "https://acme.atlassian.net/wiki/", Own: true, Username: "c@b.co"},
	})
	if next.Username != "new@acme.co" || next.APIToken != "keep-me" || next.Jira.URL != "https://acme.atlassian.net" {
		t.Fatalf("shared = %+v", next)
	}
	if next.Confluence.URL != "https://acme.atlassian.net/wiki" || next.Confluence.APIToken != "other" || next.Confluence.Username != "c@b.co" {
		t.Fatalf("confluence = %+v", next.Confluence)
	}
	cleared := Apply(next, Atlassian{})
	if !cleared.Empty() {
		t.Fatalf("cleared = %+v", cleared)
	}
}

func TestAtlassianPlaceRoundTrip(t *testing.T) {
	cfg := Atlassian{
		Enabled:  true,
		Username: "a@b.co",
		APIToken: "secret-token",
		Jira:     Service{Enabled: true, URL: "https://acme.atlassian.net", BoardID: "12", BoardName: "KIKI"},
		Confluence: Service{
			Enabled: true, URL: "https://acme.atlassian.net/wiki",
			SpaceID: "111", SpaceKey: "ENG", SpaceName: "Engineering",
			ParentID: "99", ParentTitle: "Requirements",
		},
	}
	raw, err := yaml.Marshal(cfg)
	if err != nil {
		t.Fatal(err)
	}
	var got Atlassian
	if err := yaml.Unmarshal(raw, &got); err != nil {
		t.Fatal(err)
	}
	got = got.Normalized()
	if got.Jira.BoardID != "12" || got.Jira.BoardName != "KIKI" || got.Confluence.SpaceKey != "ENG" || got.Confluence.ParentTitle != "Requirements" {
		t.Fatalf("got = %+v\n%s", got, raw)
	}
}

func TestValidateRejectsPartialProduct(t *testing.T) {
	err := (Atlassian{Enabled: true, Jira: Service{Enabled: true, URL: "https://acme.atlassian.net"}}).Validate()
	if err == nil || !strings.Contains(err.Error(), "Atlassian needs an email") {
		t.Fatalf("err = %v", err)
	}
	err = (Atlassian{Enabled: true, Jira: Service{Enabled: true, URL: "notaurl", Own: true, Username: "a@b.co", APIToken: "t"}}).Validate()
	if err == nil || !strings.Contains(err.Error(), "http or https") {
		t.Fatalf("err = %v", err)
	}
	off := Atlassian{Jira: Service{URL: "https://acme.atlassian.net"}}
	if err := off.Validate(); err != nil {
		t.Fatal(err)
	}
	ok := Atlassian{Enabled: true, Confluence: Service{Enabled: true, URL: "https://wiki.example.com", Own: true, Username: "a@b.co", APIToken: "pat"}}
	if err := ok.Validate(); err != nil {
		t.Fatal(err)
	}
}

func TestEnsureCursorWritesAndSkips(t *testing.T) {
	withUVX(t)
	home := t.TempDir()
	cfg := sampleAtlassian()
	if err := Ensure("cursor", "", home, "kiki", cfg); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(home, ".cursor", "mcp.json")
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if info.Mode().Perm() != 0o600 {
		t.Fatalf("mode = %o", info.Mode().Perm())
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	if !strings.Contains(text, `"mcp-atlassian-kiki"`) || !strings.Contains(text, "mcp-atlassian") || !strings.Contains(text, "secret-jira") || !strings.Contains(text, "secret-wiki") {
		t.Fatalf("mcp.json =\n%s", text)
	}
	before := text
	if err := Ensure("cursor", "", home, "kiki", cfg); err != nil {
		t.Fatal(err)
	}
	again, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(again) != before {
		t.Fatalf("second ensure rewrote the file:\n%s", again)
	}
}

func TestEnsureCursorKeepsMatchingKey(t *testing.T) {
	home := t.TempDir()
	path := filepath.Join(home, ".cursor", "mcp.json")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	original := []byte("{\"mcpServers\":{\"mcp-atlassian-kiki\":{\"command\":\"custom\"},\"jira\":{\"command\":\"other\"}}}\n")
	if err := os.WriteFile(path, original, 0o644); err != nil {
		t.Fatal(err)
	}
	lookPath = func(string) (string, error) { return "", os.ErrNotExist }
	t.Cleanup(func() { lookPath = findUVX })
	if err := Ensure("cursor", "", home, "kiki", sampleAtlassian()); err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != string(original) {
		t.Fatalf("file changed:\n%s", got)
	}
}

func TestEnsureCursorAddsWorkspaceKeyBesideOtherServers(t *testing.T) {
	withUVX(t)
	home := t.TempDir()
	path := filepath.Join(home, ".cursor", "mcp.json")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	original := []byte("{\"mcpServers\":{\"jira\":{\"command\":\"custom\"},\"mcp-atlassian-other\":{\"command\":\"other\"}}}\n")
	if err := os.WriteFile(path, original, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := Ensure("cursor", "", home, "kiki", sampleAtlassian()); err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(got)
	if !strings.Contains(text, `"jira"`) || !strings.Contains(text, `"mcp-atlassian-other"`) || !strings.Contains(text, `"mcp-atlassian-kiki"`) || !strings.Contains(text, "secret-jira") {
		t.Fatalf("mcp.json =\n%s", text)
	}
}

func TestEnsureRequiresUVXBeforeWrite(t *testing.T) {
	home := t.TempDir()
	lookPath = func(string) (string, error) { return "", os.ErrNotExist }
	t.Cleanup(func() { lookPath = findUVX })
	err := Ensure("gemini", "", home, "kiki", sampleAtlassian())
	if err == nil || !strings.Contains(err.Error(), "uvx is not installed") {
		t.Fatalf("err = %v", err)
	}
	if _, statErr := os.Stat(filepath.Join(home, ".gemini", "settings.json")); !os.IsNotExist(statErr) {
		t.Fatal("wrote Gemini settings without uvx")
	}
}

func TestEnsureGeminiPreservesOtherKeys(t *testing.T) {
	withUVX(t)
	home := t.TempDir()
	path := filepath.Join(home, ".gemini", "settings.json")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	original := []byte("{\n  \"theme\": \"dark\",\n  \"mcpServers\": {\"github\":{\"command\":\"echo\",\"args\":[\"hi\"]}}\n}\n")
	if err := os.WriteFile(path, original, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := Ensure("gemini", "", home, "kiki", sampleAtlassian()); err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(got)
	if !strings.Contains(text, `"theme": "dark"`) || !strings.Contains(text, `"command":"echo"`) || !strings.Contains(text, `"mcp-atlassian-kiki"`) {
		t.Fatalf("settings =\n%s", text)
	}
}

func TestEnsureClaudeSkipsProjectServerAndPreservesFile(t *testing.T) {
	withUVX(t)
	home := t.TempDir()
	work := t.TempDir()
	path := filepath.Join(home, ".claude.json")
	original := []byte("{\"num\":1,\"projects\":{\"" + work + "\":{\"mcpServers\":{\"mcp-atlassian-kiki\":{\"command\":\"uvx\",\"args\":[\"mcp-atlassian\"]}}}}}\n")
	if err := os.WriteFile(path, original, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := Ensure("claude", work, home, "kiki", sampleAtlassian()); err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != string(original) {
		t.Fatalf("claude.json changed:\n%s", got)
	}
	if err := Ensure("claude", filepath.Join(work, "other"), home, "kiki", sampleAtlassian()); err != nil {
		t.Fatal(err)
	}
	got, err = os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(got)
	if !strings.Contains(text, `"num": 1`) || !strings.Contains(text, `"mcp-atlassian-kiki"`) || !strings.Contains(text, `"type":"stdio"`) || !strings.Contains(text, "secret-jira") {
		t.Fatalf("claude.json =\n%s", text)
	}
}

func TestEnsureCodexAppends(t *testing.T) {
	withUVX(t)
	home := t.TempDir()
	path := filepath.Join(home, ".codex", "config.toml")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("model = \"gpt\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	cfg := sampleAtlassian()
	cfg.APIToken = `tok"en`
	if err := Ensure("codex", "", home, "kiki", cfg); err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(got)
	if !strings.HasPrefix(text, "model = \"gpt\"\n") || !strings.Contains(text, "[mcp_servers.mcp-atlassian-kiki]") || !strings.Contains(text, `JIRA_API_TOKEN = "tok\"en"`) {
		t.Fatalf("config =\n%s", text)
	}
	if err := Ensure("codex", "", home, "kiki", cfg); err != nil {
		t.Fatal(err)
	}
	again, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(again) != text {
		t.Fatalf("second ensure rewrote Codex config:\n%s", again)
	}
}

func TestEnsureSkipsWhenTheLaunchFolderAlreadyHasIt(t *testing.T) {
	home := t.TempDir()
	work := t.TempDir()
	path := filepath.Join(work, ".cursor", "mcp.json")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(`{"mcpServers":{"mcp-atlassian-kiki":{"command":"uvx","args":["mcp-atlassian"]}}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	lookPath = func(string) (string, error) { return "", os.ErrNotExist }
	t.Cleanup(func() { lookPath = findUVX })
	if err := Ensure("cursor", work, home, "kiki", sampleAtlassian()); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(home, ".cursor", "mcp.json")); !os.IsNotExist(err) {
		t.Fatal("wrote user MCP settings even though the launch folder already has Atlassian")
	}
}

func TestEnsureEmptyAndUnknownTool(t *testing.T) {
	home := t.TempDir()
	if err := Ensure("cursor", "", home, "kiki", Atlassian{}); err != nil {
		t.Fatal(err)
	}
	disabled := sampleAtlassian()
	disabled.Enabled = false
	if err := Ensure("cursor", "", home, "kiki", disabled); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(home, ".cursor")); !os.IsNotExist(err) {
		t.Fatal("empty setup created a Cursor folder")
	}
	err := Ensure("cursor", "", home, "Kiki", sampleAtlassian())
	if err == nil || !strings.Contains(err.Error(), "workspace must be lowercase") {
		t.Fatalf("err = %v", err)
	}
	err = Ensure("other", "", home, "kiki", sampleAtlassian())
	if err == nil || !strings.Contains(err.Error(), "AI Tool is not supported") {
		t.Fatalf("err = %v", err)
	}
}

func TestLegacyFileUsesSharedCredentials(t *testing.T) {
	raw := []byte(`
jira:
  url: https://acme.atlassian.net/
  username: a@acme.co
  api_token: secret-jira
confluence:
  url: https://acme.atlassian.net/wiki/
  username: wiki@acme.co
  api_token: secret-wiki
confluence_same_as_jira: false
`)
	var cfg Atlassian
	if err := yaml.Unmarshal(raw, &cfg); err != nil {
		t.Fatal(err)
	}
	if !cfg.Enabled || cfg.Username != "a@acme.co" || cfg.APIToken != "secret-jira" {
		t.Fatalf("shared = %+v", cfg)
	}
	if !cfg.Jira.Enabled || cfg.Jira.Own || cfg.Jira.URL != "https://acme.atlassian.net" {
		t.Fatalf("jira = %+v", cfg.Jira)
	}
	if !cfg.Confluence.Enabled || !cfg.Confluence.Own || cfg.Confluence.Username != "wiki@acme.co" || cfg.Confluence.APIToken != "secret-wiki" {
		t.Fatalf("confluence = %+v", cfg.Confluence)
	}
	same := []byte(`
jira:
  url: https://acme.atlassian.net
  username: a@acme.co
  api_token: secret-jira
confluence:
  url: https://acme.atlassian.net/wiki
confluence_same_as_jira: true
`)
	cfg = Atlassian{}
	if err := yaml.Unmarshal(same, &cfg); err != nil {
		t.Fatal(err)
	}
	if !cfg.Confluence.Enabled || cfg.Confluence.Own || cfg.Confluence.APIToken != "" || cfg.APIToken != "secret-jira" {
		t.Fatalf("same = %+v", cfg)
	}
}

func TestDisabledSetupStaysOff(t *testing.T) {
	cfg := sampleAtlassian()
	cfg.Enabled = false
	raw, err := yaml.Marshal(cfg)
	if err != nil {
		t.Fatal(err)
	}
	var got Atlassian
	if err := yaml.Unmarshal(raw, &got); err != nil {
		t.Fatal(err)
	}
	if got.Enabled || got.APIToken != "secret-jira" || !got.Jira.Enabled || got.Ready() {
		t.Fatalf("got = %+v\n%s", got, raw)
	}
}

func sampleAtlassian() Atlassian {
	return Atlassian{
		Enabled:  true,
		Username: "a@acme.co",
		APIToken: "secret-jira",
		Jira:     Service{Enabled: true, URL: "https://acme.atlassian.net"},
		Confluence: Service{
			Enabled: true, URL: "https://acme.atlassian.net/wiki", Own: true, Username: "a@acme.co", APIToken: "secret-wiki",
		},
	}
}

func withUVX(t *testing.T) {
	t.Helper()
	prev := lookPath
	lookPath = func(string) (string, error) { return "uvx", nil }
	t.Cleanup(func() { lookPath = prev })
}
