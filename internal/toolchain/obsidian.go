package toolchain

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"
)

// ObsidianConfigFile is the Obsidian vault list. Tests replace it.
var ObsidianConfigFile = defaultObsidianConfigFile

// VaultName is the Obsidian vault name for a folder: its last path segment.
// The CLI parameter vault= takes this name, not the absolute path.
func VaultName(folder string) string {
	return filepath.Base(filepath.Clean(folder))
}

// ObsidianStatus is `obsidian vault=<name> vault info=path`.
// vault= is the first parameter so the CLI reads this vault, not another open one.
func ObsidianStatus(folder string) []string {
	return []string{"obsidian", "vault=" + VaultName(folder), "vault", "info=path"}
}

// RelaunchObsidian quits Obsidian and opens folder. Tests replace it.
var RelaunchObsidian = relaunchObsidian

// OpenVault opens folder in Obsidian and waits until the CLI reports that path.
// The CLI only answers when that vault is open. "Vault not found." means no
// vault window is open, so this opens the folder and, if the CLI still cannot
// see it, relaunches Obsidian on that folder.
func OpenVault(cli, folder string) error {
	folder, err := filepath.Abs(folder)
	if err != nil {
		return err
	}
	if strings.TrimSpace(cli) == "" {
		return fmt.Errorf("Obsidian CLI is not set")
	}
	if ok, _ := vaultReady(cli, folder); ok {
		return nil
	}
	code := openVaultCode(folder)
	deadline := time.Now().Add(20 * time.Second)
	relaunched := false
	var last string
	for {
		out, evalErr := runCLI(cli, "eval", "code="+code)
		last = preferText(last, out)
		if evalErr == nil && evalTrue(out) {
			if ok, status := vaultReady(cli, folder); ok {
				return nil
			} else {
				last = preferText(last, status)
			}
		}
		opened, openErr := runCLI(cli, folder)
		if openErr == nil {
			last = preferText(last, opened)
			if ok, status := vaultReady(cli, folder); ok {
				return nil
			} else {
				last = preferText(last, status)
			}
		}
		if !relaunched && strings.Contains(last, "Vault not found") {
			relaunched = true
			if err := RelaunchObsidian(folder); err != nil {
				last = preferText(last, err.Error())
			}
			if ok, status := vaultReady(cli, folder); ok {
				return nil
			} else {
				last = preferText(last, status)
			}
		}
		if time.Now().After(deadline) {
			break
		}
		time.Sleep(700 * time.Millisecond)
	}
	last = strings.TrimSpace(last)
	if last == "" {
		return fmt.Errorf("Obsidian did not open %s", folder)
	}
	return fmt.Errorf("%s", firstOutputLine(last))
}

func preferText(current, next string) string {
	next = strings.TrimSpace(next)
	if next == "" {
		return current
	}
	return next
}

func vaultReady(cli, folder string) (bool, string) {
	status, err := runCLI(cli, "vault="+VaultName(folder), "vault", "info=path")
	if err == nil && statusMatches(status, folder) {
		return true, status
	}
	return false, status
}

func relaunchObsidian(folder string) error {
	if runtime.GOOS != "darwin" {
		return fmt.Errorf("open %s in Obsidian", folder)
	}
	_ = exec.Command("osascript", "-e", `tell application "Obsidian" to quit`).Run()
	deadline := time.Now().Add(8 * time.Second)
	for time.Now().Before(deadline) {
		if exec.Command("pgrep", "-x", "Obsidian").Run() != nil {
			break
		}
		time.Sleep(300 * time.Millisecond)
	}
	return exec.Command("open", "-a", "Obsidian", folder).Run()
}

func openVaultCode(folder string) string {
	path, _ := json.Marshal(folder)
	return "window.electron.ipcRenderer.sendSync(\"vault-open\"," + string(path) + ",false)"
}

func evalTrue(text string) bool {
	for _, line := range strings.Split(text, "\n") {
		switch strings.TrimSpace(line) {
		case "=> true", "true":
			return true
		}
	}
	return false
}

func statusMatches(output, folder string) bool {
	for _, line := range strings.Split(output, "\n") {
		line = strings.TrimSpace(line)
		if line != "" && samePath(line, folder) {
			return true
		}
	}
	return false
}

func runCLI(cli string, args ...string) (string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, cli, args...)
	out, err := cmd.CombinedOutput()
	return strings.TrimSpace(string(out)), err
}

func firstOutputLine(text string) string {
	for _, line := range strings.Split(text, "\n") {
		line = strings.TrimSpace(line)
		if line != "" {
			return line
		}
	}
	return ""
}

// EnsureVault makes folder an Obsidian vault and registers it in the Obsidian vault list.
// The returned note is safe to show in the command log.
func EnsureVault(folder string) (string, error) {
	folder, err := filepath.Abs(folder)
	if err != nil {
		return "", err
	}
	info, err := os.Stat(folder)
	if err != nil || !info.IsDir() {
		return "", fmt.Errorf("folder %s does not exist", folder)
	}
	if err := os.MkdirAll(filepath.Join(folder, ".obsidian"), 0o755); err != nil {
		return "", err
	}
	marker := filepath.Join(folder, ".obsidian", "app.json")
	if _, err := os.Stat(marker); os.IsNotExist(err) {
		if err := os.WriteFile(marker, []byte("{}\n"), 0o644); err != nil {
			return "", err
		}
	} else if err != nil {
		return "", err
	}
	return registerVault(folder)
}

func defaultObsidianConfigFile() string {
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	switch runtime.GOOS {
	case "darwin":
		return filepath.Join(home, "Library", "Application Support", "obsidian", "obsidian.json")
	case "windows":
		if app := os.Getenv("APPDATA"); app != "" {
			return filepath.Join(app, "obsidian", "obsidian.json")
		}
		return filepath.Join(home, "AppData", "Roaming", "obsidian", "obsidian.json")
	default:
		if xdg := os.Getenv("XDG_CONFIG_HOME"); xdg != "" {
			return filepath.Join(xdg, "obsidian", "obsidian.json")
		}
		return filepath.Join(home, ".config", "obsidian", "obsidian.json")
	}
}

func registerVault(folder string) (string, error) {
	path := ObsidianConfigFile()
	if strings.TrimSpace(path) == "" {
		return "", fmt.Errorf("Obsidian config path is not set")
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return "", err
	}
	doc := map[string]any{}
	mode := os.FileMode(0o644)
	if raw, err := os.ReadFile(path); err == nil {
		if info, statErr := os.Stat(path); statErr == nil {
			mode = info.Mode().Perm()
		}
		if len(strings.TrimSpace(string(raw))) > 0 {
			if err := json.Unmarshal(raw, &doc); err != nil {
				return "", fmt.Errorf("obsidian config: %w", err)
			}
		}
	} else if !os.IsNotExist(err) {
		return "", err
	}
	vaults, ok := doc["vaults"].(map[string]any)
	if !ok || vaults == nil {
		if _, exists := doc["vaults"]; exists && doc["vaults"] != nil {
			return "", fmt.Errorf("obsidian config: vaults is not an object")
		}
		vaults = map[string]any{}
		doc["vaults"] = vaults
	}
	for _, item := range vaults {
		entry, ok := item.(map[string]any)
		if !ok {
			continue
		}
		existing, _ := entry["path"].(string)
		if samePath(existing, folder) {
			return "Obsidian vault already registered at " + folder, nil
		}
	}
	id, err := vaultID()
	if err != nil {
		return "", err
	}
	vaults[id] = map[string]any{
		"path": folder,
		"ts":   time.Now().UnixMilli(),
	}
	out, err := json.MarshalIndent(doc, "", "  ")
	if err != nil {
		return "", err
	}
	out = append(out, '\n')
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, out, mode); err != nil {
		return "", err
	}
	if err := os.Rename(tmp, path); err != nil {
		_ = os.Remove(tmp)
		return "", err
	}
	return "Registered Obsidian vault at " + folder, nil
}

func vaultID() (string, error) {
	buf := make([]byte, 8)
	if _, err := rand.Read(buf); err != nil {
		return "", err
	}
	return hex.EncodeToString(buf), nil
}

func samePath(a, b string) bool {
	a = filepath.Clean(a)
	b = filepath.Clean(b)
	if pathEqual(a, b) {
		return true
	}
	ar, errA := filepath.EvalSymlinks(a)
	br, errB := filepath.EvalSymlinks(b)
	if errA != nil || errB != nil {
		return false
	}
	return pathEqual(filepath.Clean(ar), filepath.Clean(br))
}

func pathEqual(a, b string) bool {
	if runtime.GOOS == "windows" || runtime.GOOS == "darwin" {
		return strings.EqualFold(a, b)
	}
	return a == b
}

// ObsidianCLICandidates lists bundled CLI binaries. Tests replace it.
var ObsidianCLICandidates = defaultObsidianCLICandidates

// ObsidianLinkDir is where Set up places an `obsidian` command. Tests replace it.
var ObsidianLinkDir = defaultObsidianLinkDir

// BundledObsidianCLI is the obsidian-cli binary inside the Obsidian app.
func BundledObsidianCLI() string {
	for _, path := range ObsidianCLICandidates() {
		info, err := os.Stat(path)
		if err != nil || info.IsDir() || info.Mode()&0o111 == 0 {
			continue
		}
		return path
	}
	return ""
}

// InstallObsidianCommand makes `obsidian` available without administrator rights.
// It links the bundled CLI into ~/.local/bin when that name is not already there.
func InstallObsidianCommand() (string, error) {
	bundled := BundledObsidianCLI()
	if bundled == "" {
		return "", fmt.Errorf("Obsidian is not installed. Install it from https://obsidian.md/ and turn on Settings → General → Command line interface")
	}
	dir := ObsidianLinkDir()
	if strings.TrimSpace(dir) == "" {
		return bundled, nil
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return bundled, nil
	}
	dest := filepath.Join(dir, "obsidian")
	if samePath(dest, bundled) {
		return dest, nil
	}
	if _, err := os.Lstat(dest); err == nil {
		return bundled, nil
	}
	if err := os.Symlink(bundled, dest); err != nil {
		return bundled, nil
	}
	return dest, nil
}

func defaultObsidianLinkDir() string {
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	return filepath.Join(home, ".local", "bin")
}

func defaultObsidianCLICandidates() []string {
	home, _ := os.UserHomeDir()
	switch runtime.GOOS {
	case "darwin":
		return []string{
			"/Applications/Obsidian.app/Contents/MacOS/obsidian-cli",
			filepath.Join(home, "Applications", "Obsidian.app", "Contents", "MacOS", "obsidian-cli"),
		}
	case "windows":
		return []string{
			filepath.Join(os.Getenv("LOCALAPPDATA"), "Programs", "Obsidian", "obsidian-cli.exe"),
			filepath.Join(os.Getenv("ProgramFiles"), "Obsidian", "obsidian-cli.exe"),
		}
	default:
		return []string{
			"/usr/local/bin/obsidian-cli",
			"/usr/bin/obsidian-cli",
			filepath.Join(home, ".local", "share", "obsidian", "obsidian-cli"),
		}
	}
}
