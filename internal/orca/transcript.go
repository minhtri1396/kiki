package orca

import (
	"regexp"
	"strings"
	"unicode"
)

// Ask is a question an agent left for the user.
type Ask struct {
	Prompt  string
	Options []string
}

type lineKind int

const (
	kindAnswer lineKind = iota
	kindThinking
)

type spoken struct {
	kind lineKind
	text string
}

// Transcript keeps an agent's thinking and replies, and a question when the agent is waiting.
func Transcript(screen string) (string, Ask) {
	var lines []spoken
	thinking := false
	for _, line := range strings.Split(screen, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			thinking = false
			continue
		}
		if chromeLine(line) {
			continue
		}
		if isThinkingHeader(line) {
			thinking = true
			continue
		}
		kind := kindAnswer
		if thinking {
			kind = kindThinking
		}
		lines = append(lines, spoken{kind, line})
		if kind == kindAnswer {
			thinking = false
		}
	}
	ask, drop := questionAtEnd(lines)
	var thinkingLines, answerLines []string
	for i, line := range lines {
		if drop[i] {
			continue
		}
		if line.kind == kindThinking {
			thinkingLines = append(thinkingLines, line.text)
			continue
		}
		if line.kind == kindAnswer {
			answerLines = append(answerLines, line.text)
		}
	}
	var b strings.Builder
	if text := strings.Join(thinkingLines, "\n"); text != "" {
		b.WriteString("Thinking\n")
		b.WriteString(text)
	}
	if text := strings.Join(answerLines, "\n"); text != "" {
		if b.Len() > 0 {
			b.WriteString("\n\n")
		}
		b.WriteString(text)
	}
	return strings.TrimSpace(b.String()), ask
}

func questionAtEnd(lines []spoken) (Ask, map[int]bool) {
	if len(lines) == 0 {
		return Ask{}, nil
	}
	drop := map[int]bool{}
	i := len(lines) - 1
	var options []string
	for i >= 0 {
		opt, ok := optionLine(lines[i].text)
		if !ok {
			break
		}
		options = append([]string{opt}, options...)
		i--
	}
	if i < 0 || !strings.HasSuffix(lines[i].text, "?") {
		return Ask{}, nil
	}
	if len(options) == 0 && !userQuestion(lines[i].text) {
		return Ask{}, nil
	}
	drop[i] = true
	for j := i + 1; j < len(lines); j++ {
		drop[j] = true
	}
	return Ask{Prompt: lines[i].text, Options: options}, drop
}

func chromeLine(line string) bool {
	lower := strings.ToLower(line)
	switch {
	case strings.Contains(lower, "pasted text"),
		strings.Contains(lower, "run everything"),
		strings.Contains(lower, "npmrc"),
		strings.Contains(lower, "nvm "),
		strings.HasPrefix(lower, "tip:"),
		lower == "cursor agent",
		lower == "auto",
		strings.HasPrefix(line, "~/"),
		strings.Contains(line, "@") && strings.Contains(line, "cursor-agent"):
		return true
	}
	if versionLine.MatchString(line) || shellLine.MatchString(line) {
		return true
	}
	for _, r := range line {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			return false
		}
	}
	return true
}

func isThinkingHeader(line string) bool {
	switch strings.ToLower(strings.Trim(line, ": ")) {
	case "thinking", "thought", "thoughts", "reasoning":
		return true
	}
	return false
}

var (
	optionPattern = regexp.MustCompile(`^(?:[0-9]+[.)]|[-*•]|[❯›])\s+(.+)$`)
	versionLine   = regexp.MustCompile(`^v\d{4}\.`)
	shellLine     = regexp.MustCompile(`^[^@\s]+@\S+`)
	userAsk       = regexp.MustCompile(`(?i)^(should i|do you|would you|can i|please|which|what should|could you|confirm)\b`)
)

func optionLine(line string) (string, bool) {
	match := optionPattern.FindStringSubmatch(line)
	if match == nil {
		return "", false
	}
	text := strings.TrimSpace(match[1])
	if text == "" || strings.HasSuffix(text, "?") {
		return "", false
	}
	return text, true
}

func userQuestion(line string) bool {
	return userAsk.MatchString(line)
}
