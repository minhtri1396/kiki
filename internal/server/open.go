package server

import (
	"errors"
	"net/url"
	"os/exec"
	"regexp"
	"runtime"
	"strings"
)

var jiraKeyPattern = regexp.MustCompile(`^[A-Z][A-Z0-9]{1,9}-[0-9]+$`)

// startBrowser opens a page in the system browser. Tests replace it.
var startBrowser = openBrowser

// browserPages are the only addresses the console may open in the system browser.
var indexPages = map[string]struct{}{
	"https://obsidian.md/help/cli":                                {},
	"https://github.com/colbymchenry/codegraph":                   {},
	"https://id.atlassian.com/manage-profile/security/api-tokens": {},
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

func openBrowser(page string) error {
	cmd, err := pageCommand(page)
	if err != nil {
		return err
	}
	return cmd.Run()
}

// jiraBrowsePage is the Jira issue page for key on the saved site.
func jiraBrowsePage(site, key string) (string, error) {
	key = strings.ToUpper(strings.TrimSpace(key))
	if !jiraKeyPattern.MatchString(key) {
		return "", errors.New("That ticket is not on a group.")
	}
	parsed, err := url.Parse(strings.TrimSpace(site))
	if err != nil || parsed.Host == "" || (parsed.Scheme != "https" && parsed.Scheme != "http") || parsed.User != nil || parsed.RawQuery != "" || parsed.Fragment != "" {
		return "", errors.New("Set the Jira site URL in Settings.")
	}
	path := strings.TrimSuffix(parsed.EscapedPath(), "/")
	parsed.Path = path + "/browse/" + key
	parsed.RawPath = ""
	parsed.RawQuery = ""
	parsed.Fragment = ""
	return parsed.String(), nil
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
