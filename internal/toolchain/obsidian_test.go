package toolchain

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestEnsureVaultRegistersOnce(t *testing.T) {
	dir := t.TempDir()
	vault := filepath.Join(dir, "docs")
	if err := os.Mkdir(vault, 0o755); err != nil {
		t.Fatal(err)
	}
	cfg := filepath.Join(dir, "obsidian.json")
	seed := []byte("{\n  \"theme\": \"moonstone\",\n  \"vaults\": {\n    \"abc\": {\"path\": \"/other\", \"ts\": 1}\n  }\n}\n")
	if err := os.WriteFile(cfg, seed, 0o600); err != nil {
		t.Fatal(err)
	}
	prev := ObsidianConfigFile
	ObsidianConfigFile = func() string { return cfg }
	t.Cleanup(func() { ObsidianConfigFile = prev })

	note, err := EnsureVault(vault)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(note, "Registered") {
		t.Fatalf("note = %q", note)
	}
	marker, err := os.ReadFile(filepath.Join(vault, ".obsidian", "app.json"))
	if err != nil {
		t.Fatal(err)
	}
	if string(marker) != "{}\n" {
		t.Fatalf("app.json = %q", marker)
	}

	again, err := EnsureVault(vault)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(again, "already registered") {
		t.Fatalf("second note = %q", again)
	}
	raw, err := os.ReadFile(cfg)
	if err != nil {
		t.Fatal(err)
	}
	var doc struct {
		Theme  string `json:"theme"`
		Vaults map[string]struct {
			Path string `json:"path"`
		} `json:"vaults"`
	}
	if err := json.Unmarshal(raw, &doc); err != nil {
		t.Fatal(err)
	}
	if doc.Theme != "moonstone" {
		t.Fatalf("theme = %q", doc.Theme)
	}
	if len(doc.Vaults) != 2 {
		t.Fatalf("vaults = %#v", doc.Vaults)
	}
	found := false
	for _, item := range doc.Vaults {
		if item.Path == vault {
			found = true
		}
	}
	if !found {
		t.Fatalf("vault path missing: %#v", doc.Vaults)
	}
	info, err := os.Stat(cfg)
	if err != nil {
		t.Fatal(err)
	}
	if info.Mode().Perm() != 0o600 {
		t.Fatalf("mode = %o", info.Mode().Perm())
	}
}

func TestInstallObsidianCommandLinksBundle(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "obsidian-cli")
	if err := os.WriteFile(bin, []byte("#!/bin/sh\nexit 0\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(dir, "bin")
	prevCandidates := ObsidianCLICandidates
	prevLink := ObsidianLinkDir
	ObsidianCLICandidates = func() []string { return []string{bin} }
	ObsidianLinkDir = func() string { return link }
	t.Cleanup(func() {
		ObsidianCLICandidates = prevCandidates
		ObsidianLinkDir = prevLink
	})

	got, err := InstallObsidianCommand()
	if err != nil {
		t.Fatal(err)
	}
	if got != filepath.Join(link, "obsidian") {
		t.Fatalf("cli = %s", got)
	}
	target, err := os.Readlink(got)
	if err != nil {
		t.Fatal(err)
	}
	if target != bin {
		t.Fatalf("link target = %s", target)
	}
	again, err := InstallObsidianCommand()
	if err != nil {
		t.Fatal(err)
	}
	if again != got {
		t.Fatalf("second cli = %s", again)
	}
}

func TestInstallObsidianCommandMissing(t *testing.T) {
	prev := ObsidianCLICandidates
	ObsidianCLICandidates = func() []string { return nil }
	t.Cleanup(func() { ObsidianCLICandidates = prev })
	if _, err := InstallObsidianCommand(); err == nil {
		t.Fatal("expected an error when Obsidian is not installed")
	}
}

func TestObsidianStatusTargetsFolder(t *testing.T) {
	args := ObsidianStatus("/tmp/docs")
	if len(args) != 4 || args[0] != "obsidian" || args[1] != "vault=docs" || args[2] != "vault" || args[3] != "info=path" {
		t.Fatalf("args = %#v", args)
	}
	if VaultName("/tmp/my docs/") != "my docs" {
		t.Fatalf("name = %q", VaultName("/tmp/my docs/"))
	}
}

func TestOpenVaultCodeQuotesPath(t *testing.T) {
	code := openVaultCode(`/tmp/my docs`)
	if !strings.Contains(code, `sendSync("vault-open","/tmp/my docs",false)`) {
		t.Fatalf("code = %s", code)
	}
}

func TestOpenVaultWaitsUntilCLIReportsPath(t *testing.T) {
	dir := t.TempDir()
	vault := filepath.Join(dir, "docs")
	if err := os.Mkdir(vault, 0o755); err != nil {
		t.Fatal(err)
	}
	script := filepath.Join(dir, "obsidian")
	body := "#!/bin/sh\ncase \"$1\" in\n  eval) echo '=> true' ;;\n  *) echo \"" + vault + "\" ;;\nesac\n"
	if err := os.WriteFile(script, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := OpenVault(script, vault); err != nil {
		t.Fatal(err)
	}
}

func TestOpenVaultRelaunchesWhenCLICannotSeeVault(t *testing.T) {
	dir := t.TempDir()
	vault := filepath.Join(dir, "docs")
	if err := os.Mkdir(vault, 0o755); err != nil {
		t.Fatal(err)
	}
	ready := filepath.Join(dir, "ready")
	script := filepath.Join(dir, "obsidian")
	body := "#!/bin/sh\nif [ -f \"" + ready + "\" ]; then echo \"" + vault + "\"; else echo 'Vault not found.'; fi\n"
	if err := os.WriteFile(script, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
	prev := RelaunchObsidian
	RelaunchObsidian = func(folder string) error {
		if folder != vault {
			t.Errorf("relaunch folder = %s", folder)
		}
		return os.WriteFile(ready, []byte("1"), 0o644)
	}
	t.Cleanup(func() { RelaunchObsidian = prev })
	if err := OpenVault(script, vault); err != nil {
		t.Fatal(err)
	}
}
