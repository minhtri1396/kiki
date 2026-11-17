package orca

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"time"
)

// TerminalView is the rendered screen of one Orca terminal.
type TerminalView struct {
	Handle string
	Status string
	Log    string
	// Draft is text sitting in the agent's input, not yet submitted.
	Draft string
}

// promptSubmitWait is how long to watch for a prompt left in the input.
var promptSubmitWait = 20 * time.Second

// ReadTerminal reads what an Orca terminal is showing, and whether it is still running.
func (c *Client) ReadTerminal(ctx context.Context, handle string) (TerminalView, error) {
	handle = strings.TrimSpace(handle)
	if handle == "" {
		return TerminalView{}, fmt.Errorf("terminal handle is required")
	}
	out, err := c.commandOutput(ctx, "terminal", "read", "--terminal", handle, "--screen", "--limit", "200", "--json")
	if err != nil {
		return TerminalView{}, err
	}
	return parseTerminalRead(out)
}

// TerminalIdle reports whether the terminal's text interface has stopped updating.
func (c *Client) TerminalIdle(ctx context.Context, handle string) (bool, error) {
	handle = strings.TrimSpace(handle)
	if handle == "" {
		return false, fmt.Errorf("terminal handle is required")
	}
	out, err := c.commandOutput(ctx, "terminal", "wait", "--terminal", handle, "--for", "tui-idle", "--timeout-ms", "400", "--json")
	if err != nil {
		return false, err
	}
	return parseTerminalIdle(out)
}

func parseTerminalRead(out []byte) (TerminalView, error) {
	var env struct {
		OK    bool `json:"ok"`
		Error struct {
			Message string `json:"message"`
		} `json:"error"`
		Result struct {
			Terminal struct {
				Handle string          `json:"handle"`
				Status string          `json:"status"`
				Tail   []string        `json:"tail"`
				Draft  json.RawMessage `json:"draft"`
			} `json:"terminal"`
		} `json:"result"`
	}
	if err := json.Unmarshal(out, &env); err != nil {
		return TerminalView{}, fmt.Errorf("read Orca terminal: %w", err)
	}
	if !env.OK {
		message := strings.TrimSpace(env.Error.Message)
		if message == "" {
			message = "Orca did not return the terminal"
		}
		return TerminalView{}, fmt.Errorf("%s", message)
	}
	status := strings.TrimSpace(env.Result.Terminal.Status)
	switch status {
	case "running", "exited", "unknown":
	default:
		status = "unknown"
	}
	return TerminalView{
		Handle: env.Result.Terminal.Handle,
		Status: status,
		Log:    strings.TrimRight(strings.Join(env.Result.Terminal.Tail, "\n"), "\n"),
		Draft:  draftText(env.Result.Terminal.Draft),
	}, nil
}

// submitPastedPrompt presses Enter when the launch left the prompt in the agent's input.
func (c *Client) submitPastedPrompt(ctx context.Context, handle string) error {
	deadline := time.Now().Add(promptSubmitWait)
	for {
		view, err := c.ReadTerminal(ctx, handle)
		if err == nil && unsentPrompt(view) {
			idle, idleErr := c.TerminalIdle(ctx, handle)
			if idleErr == nil && !idle && time.Now().Before(deadline) && ctx.Err() == nil {
				if err := waitPrompt(ctx, 400*time.Millisecond); err != nil {
					return err
				}
				continue
			}
			return c.pressEnter(ctx, handle)
		}
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if time.Now().After(deadline) {
			return nil
		}
		if err := waitPrompt(ctx, 400*time.Millisecond); err != nil {
			return err
		}
	}
}

func unsentPrompt(view TerminalView) bool {
	if strings.TrimSpace(view.Draft) != "" {
		return true
	}
	return strings.Contains(strings.ToLower(view.Log), "pasted text")
}

func draftText(raw json.RawMessage) string {
	raw = bytesTrim(raw)
	if len(raw) == 0 || string(raw) == "null" {
		return ""
	}
	var text string
	if json.Unmarshal(raw, &text) == nil {
		return text
	}
	var obj struct {
		Text    string `json:"text"`
		Value   string `json:"value"`
		Content string `json:"content"`
	}
	if json.Unmarshal(raw, &obj) == nil {
		for _, part := range []string{obj.Text, obj.Value, obj.Content} {
			if strings.TrimSpace(part) != "" {
				return part
			}
		}
	}
	return ""
}

func bytesTrim(raw []byte) []byte {
	return []byte(strings.TrimSpace(string(raw)))
}

// SendText submits a reply to the agent in an Orca terminal.
func (c *Client) SendText(ctx context.Context, handle, text string) error {
	handle = strings.TrimSpace(handle)
	text = strings.TrimSpace(text)
	if handle == "" || text == "" {
		return fmt.Errorf("Orca did not submit the prompt")
	}
	out, err := c.commandOutput(ctx, "terminal", "send", "--terminal", handle, "--text", text, "--enter", "--wait-submit", "8", "--json")
	if err != nil {
		return err
	}
	return sentOK(out)
}

func (c *Client) pressEnter(ctx context.Context, handle string) error {
	out, err := c.commandOutput(ctx, "terminal", "send", "--terminal", handle, "--enter", "--wait-submit", "8", "--json")
	if err != nil {
		return err
	}
	return sentOK(out)
}

func sentOK(out []byte) error {
	if jsonOK(out) {
		return nil
	}
	var env struct {
		Error struct {
			Code    string `json:"code"`
			Message string `json:"message"`
		} `json:"error"`
	}
	_ = json.Unmarshal(out, &env)
	if env.Error.Code == "timeout" {
		return nil
	}
	message := strings.TrimSpace(env.Error.Message)
	if message == "" {
		message = "Orca did not submit the prompt"
	}
	return fmt.Errorf("%s", message)
}

func waitPrompt(ctx context.Context, d time.Duration) error {
	timer := time.NewTimer(d)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}

func parseTerminalIdle(out []byte) (bool, error) {
	var env struct {
		OK    bool `json:"ok"`
		Error struct {
			Code    string `json:"code"`
			Message string `json:"message"`
		} `json:"error"`
	}
	if err := json.Unmarshal(out, &env); err != nil {
		return false, fmt.Errorf("read Orca terminal: %w", err)
	}
	if env.OK {
		return true, nil
	}
	if env.Error.Code == "timeout" {
		return false, nil
	}
	message := strings.TrimSpace(env.Error.Message)
	if message == "" {
		message = "Orca did not report whether the terminal is idle"
	}
	return false, fmt.Errorf("%s", message)
}
