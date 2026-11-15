package guard

import (
	"encoding/json"
	"strings"
)

// Reply is the JSON an AI tool hook prints. The shape matches that tool's hook event.
func Reply(tool, event string, allow bool, message string) string {
	if strings.TrimSpace(message) == "" && !allow {
		message = "Guardrail: KIKI blocked this action."
	}
	var raw []byte
	switch strings.ToLower(strings.TrimSpace(tool)) {
	case "claude", "codex":
		raw = claudeReply(event, allow, message)
	case "gemini":
		raw = geminiReply(allow, message)
	default:
		raw = cursorReply(allow, message)
	}
	return string(raw) + "\n"
}

func cursorReply(allow bool, message string) []byte {
	body := map[string]any{"permission": "allow"}
	if !allow {
		body["permission"] = "deny"
		body["user_message"] = message
		body["agent_message"] = message
	}
	raw, _ := json.Marshal(body)
	return raw
}

func claudeReply(event string, allow bool, message string) []byte {
	switch strings.ToLower(strings.TrimSpace(event)) {
	case "pretooluse", "permissionrequest":
		name := "PreToolUse"
		if strings.EqualFold(event, "PermissionRequest") {
			name = "PermissionRequest"
		}
		out := map[string]any{
			"hookEventName":      name,
			"permissionDecision": "allow",
		}
		if !allow {
			out["permissionDecision"] = "deny"
			out["permissionDecisionReason"] = message
		}
		raw, _ := json.Marshal(map[string]any{"hookSpecificOutput": out})
		return raw
	default:
		if allow {
			return []byte("{}")
		}
		raw, _ := json.Marshal(map[string]string{"decision": "block", "reason": message})
		return raw
	}
}

func geminiReply(allow bool, message string) []byte {
	if allow {
		return []byte("{}")
	}
	raw, _ := json.Marshal(map[string]string{"decision": "deny", "reason": message})
	return raw
}
