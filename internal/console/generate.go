package console

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"regexp"
	"strings"
	"sync"
	"time"
)

// modelID is a model name a CLI accepts, including Cursor's bracket overrides.
var modelID = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._:@+=,\[\]-]{0,160}$`)

// InputError is a problem with the text or the chosen tool, before a CLI runs.
type InputError struct{ Message string }

func (e InputError) Error() string { return e.Message }

// RewritePrompt is the instruction sent to the CLI. It asks for a rewrite of text.
func RewritePrompt(kind, subject, text string) (string, error) {
	text = strings.TrimSpace(text)
	if text == "" {
		return "", InputError{Message: "Enter some text first."}
	}
	if len(text) > 60000 {
		return "", InputError{Message: "Text is too long to generate."}
	}
	subject = oneLine(subject)
	var what string
	switch kind {
	case "prompt":
		what = "This is an agent prompt."
		if subject != "" {
			what = "This is the prompt for agent " + subject + "."
		}
	case "repo":
		if subject == "" {
			return "", InputError{Message: "Choose a repository first."}
		}
		what = "This says what the agent does in the " + subject + " repository."
	case "teamwork":
		if subject == "" {
			return "", InputError{Message: "Choose an agent first."}
		}
		what = "This says how the agent communicates and coordinates with " + subject + "."
	default:
		return "", InputError{Message: "Choose a prompt, repository note, or teamwork note."}
	}
	return what + "\n\nRewrite the text below in English. Another AI model will read it and must do exactly what it describes, so make the text concise, complete, and clear. Keep the same meaning, names, ids, and paths. Return only the rewritten text.\n\n" + text, nil
}

// previewWorkspace stands in for the temporary folder in the command shown
// before Run. The real run replaces it with the folder path.
const previewWorkspace = "kiki-preview-workspace"

// PreviewCommand is the CLI command for a rewrite, with {{prompt}} where the
// prompt is passed. Cursor also shows {{workspace}} for the temporary folder.
func PreviewCommand(tool, model, kind, subject, text string) (string, error) {
	prompt, err := RewritePrompt(kind, subject, text)
	if err != nil {
		return "", err
	}
	bin, args, err := GenerateArgs(tool, model, prompt)
	if err != nil {
		return "", err
	}
	if bin == "cursor-agent" {
		args = append(args[:len(args)-1], "--workspace", previewWorkspace, args[len(args)-1])
	}
	return FormatGenerateCommand(bin, args, prompt), nil
}

// FormatGenerateCommand is the command line shown to the user. The prompt argument
// is {{prompt}}, so the dialog does not print the prompt itself.
func FormatGenerateCommand(bin string, args []string, prompt string) string {
	parts := make([]string, 0, 1+len(args))
	parts = append(parts, quoteCommandArg(bin))
	for _, arg := range args {
		switch {
		case prompt != "" && arg == prompt:
			parts = append(parts, "{{prompt}}")
		case arg == previewWorkspace:
			parts = append(parts, "{{workspace}}")
		default:
			parts = append(parts, quoteCommandArg(arg))
		}
	}
	return strings.Join(parts, " ")
}

func quoteCommandArg(arg string) string {
	if arg == "" {
		return `""`
	}
	if strings.ContainsAny(arg, " \t\n\"'\\$`<>|&;()") {
		return "'" + strings.ReplaceAll(arg, "'", `'\''`) + "'"
	}
	return arg
}

// GenerateArgs is the CLI argv for a one-shot rewrite. The prompt is the last argument,
// except for Gemini, which takes it with --prompt.
func GenerateArgs(tool, model, prompt string) (string, []string, error) {
	tool = strings.TrimSpace(tool)
	model = strings.TrimSpace(model)
	if !contains(OrcaAgents, tool) {
		return "", nil, InputError{Message: "AI Tool is not supported"}
	}
	if model != "" && !modelID.MatchString(model) {
		return "", nil, InputError{Message: "Model is not supported"}
	}
	bin := ToolCommands[tool]
	switch tool {
	case "cursor":
		args := []string{"--print", "--output-format", "stream-json", "--stream-partial-output", "--mode", "ask", "--trust"}
		if model != "" {
			args = append(args, "--model", model)
		}
		return bin, append(args, prompt), nil
	case "claude":
		args := []string{"--print", "--bare", "--verbose", "--output-format", "stream-json", "--include-partial-messages", "--tools", "", "--no-session-persistence"}
		if model != "" {
			args = append(args, "--model", model)
		}
		return bin, append(args, prompt), nil
	case "codex":
		args := []string{"exec", "--skip-git-repo-check", "--sandbox", "read-only"}
		if model != "" {
			args = append(args, "--model", model)
		}
		return bin, append(args, prompt), nil
	case "gemini":
		args := []string{"--prompt", prompt}
		if model != "" {
			args = append(args, "--model", model)
		}
		return bin, args, nil
	default:
		return "", nil, InputError{Message: "AI Tool is not supported"}
	}
}

// GenerateEvent is one chunk of a rewrite. Log events are the model output as it
// arrives. Done carries the rewritten text and a git-style diff against the original.
type GenerateEvent struct {
	Type    string     `json:"type"`
	Channel string     `json:"channel,omitempty"`
	Text    string     `json:"text,omitempty"`
	Tool    string     `json:"tool,omitempty"`
	Model   string     `json:"model,omitempty"`
	Command string     `json:"command,omitempty"`
	Diff    []DiffLine `json:"diff,omitempty"`
	Error   string     `json:"error,omitempty"`
}

// GenerateText rewrites text with the AI tool's CLI and returns the final text.
func GenerateText(ctx context.Context, tool, model, kind, subject, text string) (string, error) {
	var result string
	err := GenerateStream(ctx, tool, model, kind, subject, text, func(ev GenerateEvent) error {
		if ev.Type == "done" {
			result = ev.Text
		}
		return nil
	})
	if err != nil {
		return "", err
	}
	if result == "" {
		return "", fmt.Errorf("the model returned no text")
	}
	return result, nil
}

// GenerateStream runs the CLI and reports log lines, then the rewritten text.
func GenerateStream(ctx context.Context, tool, model, kind, subject, text string, emit func(GenerateEvent) error) error {
	original := strings.TrimSpace(text)
	prompt, err := RewritePrompt(kind, subject, original)
	if err != nil {
		return err
	}
	bin, args, err := GenerateArgs(tool, model, prompt)
	if err != nil {
		return err
	}
	if !commandExists(bin) {
		return fmt.Errorf("%s is not installed", bin)
	}
	dir, err := os.MkdirTemp("", "kiki-generate-")
	if err != nil {
		return err
	}
	defer os.RemoveAll(dir)
	if bin == "cursor-agent" {
		args = append(args[:len(args)-1], "--workspace", dir, args[len(args)-1])
	}
	if err := emit(GenerateEvent{Type: "command", Command: FormatGenerateCommand(bin, args, prompt)}); err != nil {
		return err
	}
	if _, ok := ctx.Deadline(); !ok {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, 2*time.Minute)
		defer cancel()
	}
	run, err := startGenerateCommand(ctx, dir, bin, args...)
	if err != nil {
		if errors.Is(err, context.DeadlineExceeded) {
			return fmt.Errorf("%s took too long", bin)
		}
		return err
	}
	var mu sync.Mutex
	send := func(ev GenerateEvent) error {
		mu.Lock()
		defer mu.Unlock()
		return emit(ev)
	}
	var state streamState
	var wg sync.WaitGroup
	wg.Add(2)
	go func() {
		defer wg.Done()
		readGenerateLines(run.Stdout, &state, send)
	}()
	go func() {
		defer wg.Done()
		readPlainLog(run.Stderr, send)
	}()
	wg.Wait()
	waitErr := run.Wait()
	if state.failed {
		message := state.failMessage
		if message == "" {
			message = "the model failed"
		}
		return fmt.Errorf("%s", message)
	}
	if waitErr != nil && state.final == "" && state.assistant == "" && state.plain == "" {
		if errors.Is(waitErr, context.DeadlineExceeded) {
			return fmt.Errorf("%s took too long", bin)
		}
		message := oneLine(waitErr.Error())
		if len(message) > 300 {
			message = message[:300]
		}
		return fmt.Errorf("%s: %s", bin, message)
	}
	final := state.final
	if final == "" {
		final = state.assistant
	}
	if final == "" {
		final = state.plain
	}
	final = cleanGenerated(final)
	if final == "" {
		return fmt.Errorf("%s returned no text", bin)
	}
	return send(GenerateEvent{Type: "done", Text: final, Diff: TextDiff(original, final)})
}

type commandRun struct {
	Stdout io.Reader
	Stderr io.Reader
	Wait   func() error
}

// startGenerateCommand starts one CLI. Tests replace it.
var startGenerateCommand = func(ctx context.Context, dir, bin string, args ...string) (commandRun, error) {
	cmd := exec.CommandContext(ctx, bin, args...)
	cmd.Dir = dir
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return commandRun{}, err
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		return commandRun{}, err
	}
	if err := cmd.Start(); err != nil {
		message := oneLine(err.Error())
		if len(message) > 300 {
			message = message[:300]
		}
		return commandRun{}, fmt.Errorf("%s: %s", bin, message)
	}
	return commandRun{Stdout: stdout, Stderr: stderr, Wait: cmd.Wait}, nil
}

type streamState struct {
	mu          sync.Mutex
	thinking    string
	assistant   string
	plain       string
	final       string
	failed      bool
	failMessage string
}

type logPiece struct {
	text    string
	channel string
}

func readGenerateLines(r io.Reader, state *streamState, emit func(GenerateEvent) error) {
	if r == nil {
		return
	}
	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	for scanner.Scan() {
		line := scanner.Text()
		pieces, stop := state.consume(line)
		for _, piece := range pieces {
			if piece.text == "" {
				continue
			}
			if err := emit(GenerateEvent{Type: "log", Channel: piece.channel, Text: piece.text}); err != nil {
				return
			}
		}
		if stop {
			return
		}
	}
}

func readPlainLog(r io.Reader, emit func(GenerateEvent) error) {
	if r == nil {
		return
	}
	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	for scanner.Scan() {
		line := scanner.Text()
		if strings.TrimSpace(line) == "" {
			continue
		}
		if err := emit(GenerateEvent{Type: "log", Channel: "text", Text: line + "\n"}); err != nil {
			return
		}
	}
}

func (s *streamState) consume(line string) ([]logPiece, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	trimmed := strings.TrimSpace(line)
	if trimmed == "" {
		return nil, false
	}
	var event struct {
		Type     string `json:"type"`
		Subtype  string `json:"subtype"`
		Text     string `json:"text"`
		Thinking string `json:"thinking"`
		Result   string `json:"result"`
		IsError  bool   `json:"is_error"`
		Message  struct {
			Content []struct {
				Type     string `json:"type"`
				Text     string `json:"text"`
				Thinking string `json:"thinking"`
			} `json:"content"`
		} `json:"message"`
		Event struct {
			Type  string `json:"type"`
			Delta struct {
				Type     string `json:"type"`
				Text     string `json:"text"`
				Thinking string `json:"thinking"`
			} `json:"delta"`
		} `json:"event"`
		Delta struct {
			Type     string `json:"type"`
			Text     string `json:"text"`
			Thinking string `json:"thinking"`
		} `json:"delta"`
		Item struct {
			Type string `json:"type"`
			Text string `json:"text"`
		} `json:"item"`
	}
	if err := json.Unmarshal([]byte(trimmed), &event); err != nil || event.Type == "" {
		s.plain += line + "\n"
		return []logPiece{{line + "\n", "text"}}, false
	}
	switch event.Type {
	case "thinking":
		if event.Subtype == "completed" {
			return []logPiece{{"\n", "think"}}, false
		}
		body := event.Text
		if body == "" {
			body = event.Thinking
		}
		return s.grow(&s.thinking, body, "think"), false
	case "stream_event", "content_block_delta":
		delta := event.Event.Delta
		if event.Type == "content_block_delta" {
			delta = event.Delta
		}
		return s.deltaPieces(delta.Type, delta.Text, delta.Thinking), false
	case "assistant":
		var thought, answer string
		for _, block := range event.Message.Content {
			switch block.Type {
			case "thinking", "reasoning":
				if block.Thinking != "" {
					thought += block.Thinking
				} else {
					thought += block.Text
				}
			case "text", "":
				answer += block.Text
			}
		}
		pieces := s.grow(&s.thinking, thought, "think")
		return append(pieces, s.grow(&s.assistant, answer, "text")...), false
	case "item.completed":
		if event.Item.Type == "thinking" || event.Item.Type == "reasoning" {
			return s.grow(&s.thinking, event.Item.Text, "think"), false
		}
		return s.grow(&s.assistant, event.Item.Text, "text"), false
	case "result":
		s.final = event.Result
		if event.IsError || event.Subtype == "error" {
			s.failed = true
			s.failMessage = oneLine(event.Result)
		}
		return nil, false
	default:
		return nil, false
	}
}

func (s *streamState) deltaPieces(kind, text, thinking string) []logPiece {
	var pieces []logPiece
	if thinking != "" {
		pieces = append(pieces, s.grow(&s.thinking, thinking, "think")...)
	} else if kind == "thinking_delta" || kind == "reasoning_delta" {
		pieces = append(pieces, s.grow(&s.thinking, text, "think")...)
	}
	if text != "" && thinking == "" && kind != "thinking_delta" && kind != "reasoning_delta" {
		pieces = append(pieces, s.grow(&s.assistant, text, "text")...)
	}
	return pieces
}

// grow logs the new part of a streamed field. A later message that repeats the
// text already received is ignored.
func (s *streamState) grow(have *string, next, channel string) []logPiece {
	if next == "" || next == *have {
		return nil
	}
	extra := next
	if *have != "" && strings.HasPrefix(next, *have) {
		extra = next[len(*have):]
		*have = next
	} else if *have != "" && strings.HasPrefix(*have, next) {
		return nil
	} else {
		*have += next
	}
	if extra == "" {
		return nil
	}
	return []logPiece{{extra, channel}}
}

func cleanGenerated(text string) string {
	text = strings.TrimSpace(text)
	if !strings.HasPrefix(text, "```") {
		return text
	}
	rest := strings.TrimPrefix(text, "```")
	newline := strings.IndexByte(rest, '\n')
	if newline < 0 {
		return text
	}
	body := strings.TrimSpace(rest[newline+1:])
	if !strings.HasSuffix(body, "```") {
		return text
	}
	body = strings.TrimSpace(strings.TrimSuffix(body, "```"))
	if body == "" {
		return text
	}
	return body
}

func oneLine(text string) string {
	text = strings.TrimSpace(text)
	text = strings.ReplaceAll(text, "\r\n", " ")
	text = strings.ReplaceAll(text, "\n", " ")
	text = strings.ReplaceAll(text, "\r", " ")
	return strings.Join(strings.Fields(text), " ")
}
