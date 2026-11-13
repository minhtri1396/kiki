package server

import (
	"errors"
	"os/exec"
	"runtime"
	"strings"
)

// indexPages are the only addresses the console may open in the system browser.
var indexPages = map[string]struct{}{
	"https://obsidian.md/help/cli":              {},
	"https://github.com/colbymchenry/codegraph": {},
}

func openIndexPage(raw string) error {
	page := strings.TrimSpace(raw)
	if _, ok := indexPages[page]; !ok {
		return errors.New("that page cannot be opened")
	}
	cmd, err := pageCommand(page)
	if err != nil {
		return err
	}
	return cmd.Run()
}

func pageCommand(page string) (*exec.Cmd, error) {
	switch runtime.GOOS {
	case "darwin":
		return exec.Command("open", page), nil
	case "windows":
		return exec.Command("rundll32", "url.dll,FileProtocolHandler", page), nil
	default:
		if _, err := exec.LookPath("xdg-open"); err != nil {
			return nil, errors.New("no browser opener was found")
		}
		return exec.Command("xdg-open", page), nil
	}
}
