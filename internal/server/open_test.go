package server

import (
	"runtime"
	"testing"
)

func TestPageCommandOpensOnlyIndexPages(t *testing.T) {
	cmd, err := pageCommand("https://obsidian.md/help/cli")
	if err != nil {
		t.Fatal(err)
	}
	if runtime.GOOS == "darwin" && (cmd.Path == "" || len(cmd.Args) != 2 || cmd.Args[1] != "https://obsidian.md/help/cli") {
		t.Fatalf("args = %#v", cmd.Args)
	}
	if cmd.Args[len(cmd.Args)-1] != "https://obsidian.md/help/cli" {
		t.Fatalf("args = %#v", cmd.Args)
	}
	if err := openIndexPage("https://example.com"); err == nil {
		t.Fatal("expected a rejected page")
	}
	if err := openIndexPage("https://obsidian.md/help/cli/../download"); err == nil {
		t.Fatal("expected a rejected page")
	}
}
