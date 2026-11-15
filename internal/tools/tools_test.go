package tools

import "testing"

func TestCatalogIsGitHubCLIThenRTK(t *testing.T) {
	list := Catalog()
	if len(list) != 2 || list[0].ID != "github_cli" || list[0].Global || list[1].ID != "rtk" || !list[1].Global || list[1].Command != "rtk" {
		t.Fatalf("catalog = %+v", list)
	}
	if !Global("rtk") || Global("github_cli") || Global("other") {
		t.Fatal("global flag did not follow the catalog")
	}
	on := Settings{GitHubCLI: true, RTK: true}
	if on.Empty() || !on.On("github_cli") || !on.On("rtk") || on.On("other") {
		t.Fatalf("on = %+v", on)
	}
	if !(Settings{}).Empty() || !(Settings{}).GuardrailsOn() {
		t.Fatal("zero settings should keep guardrails on and store nothing")
	}
	off := false
	if (Settings{Guardrails: &off}).Empty() || (Settings{Guardrails: &off}).GuardrailsOn() {
		t.Fatal("turning guardrails off should be stored")
	}
}

func TestInstalledUsesTheCommandCheck(t *testing.T) {
	prev := commandExists
	commandExists = func(name string) bool { return name == "gh" }
	t.Cleanup(func() { commandExists = prev })
	if !Installed("gh") || Installed("rtk") || Installed("  ") {
		t.Fatal("installed check did not follow the stub")
	}
}
