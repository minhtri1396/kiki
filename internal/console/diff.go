package console

import "strings"

// DiffLine is one row of a unified diff. Kind is hunk, same, del, or add.
type DiffLine struct {
	Kind string `json:"kind"`
	Text string `json:"text"`
}

// TextDiff is a git-style unified diff of two texts, with three lines of context.
func TextDiff(before, after string) []DiffLine {
	oldLines := splitDiffLines(before)
	newLines := splitDiffLines(after)
	ops := editOps(oldLines, newLines)
	const context = 3
	changed := make([]bool, len(ops))
	for i, op := range ops {
		changed[i] = op.kind != "same"
	}
	keep := make([]bool, len(ops))
	for i := range ops {
		if !changed[i] {
			continue
		}
		from := i - context
		if from < 0 {
			from = 0
		}
		to := i + context
		if to >= len(ops) {
			to = len(ops) - 1
		}
		for j := from; j <= to; j++ {
			keep[j] = true
		}
	}
	var out []DiffLine
	for i := 0; i < len(ops); {
		if !keep[i] {
			i++
			continue
		}
		start := i
		for i < len(ops) && keep[i] {
			i++
		}
		out = append(out, hunk(ops[start:i])...)
	}
	return out
}

type diffOp struct {
	kind string
	text string
	old  int
	new  int
}

func splitDiffLines(text string) []string {
	if text == "" {
		return nil
	}
	lines := strings.Split(text, "\n")
	if len(lines) > 0 && lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}
	return lines
}

func editOps(oldLines, newLines []string) []diffOp {
	n, m := len(oldLines), len(newLines)
	dp := make([][]int, n+1)
	for i := range dp {
		dp[i] = make([]int, m+1)
	}
	for i := n - 1; i >= 0; i-- {
		for j := m - 1; j >= 0; j-- {
			if oldLines[i] == newLines[j] {
				dp[i][j] = dp[i+1][j+1] + 1
			} else if dp[i+1][j] >= dp[i][j+1] {
				dp[i][j] = dp[i+1][j]
			} else {
				dp[i][j] = dp[i][j+1]
			}
		}
	}
	ops := make([]diffOp, 0, n+m)
	oldLine, newLine := 1, 1
	i, j := 0, 0
	add := func(kind, text string) {
		ops = append(ops, diffOp{kind: kind, text: text, old: oldLine, new: newLine})
		switch kind {
		case "same":
			oldLine++
			newLine++
		case "del":
			oldLine++
		case "add":
			newLine++
		}
	}
	for i < n && j < m {
		if oldLines[i] == newLines[j] {
			add("same", oldLines[i])
			i++
			j++
			continue
		}
		if dp[i+1][j] >= dp[i][j+1] {
			add("del", oldLines[i])
			i++
			continue
		}
		add("add", newLines[j])
		j++
	}
	for i < n {
		add("del", oldLines[i])
		i++
	}
	for j < m {
		add("add", newLines[j])
		j++
	}
	return ops
}

func hunk(ops []diffOp) []DiffLine {
	if len(ops) == 0 {
		return nil
	}
	oldStart, newStart := ops[0].old, ops[0].new
	oldCount, newCount := 0, 0
	for _, op := range ops {
		switch op.kind {
		case "same":
			oldCount++
			newCount++
		case "del":
			oldCount++
		case "add":
			newCount++
		}
	}
	if oldCount == 0 {
		oldStart = 0
	}
	if newCount == 0 {
		newStart = 0
	}
	header := "@@ -" + itoa(oldStart) + "," + itoa(oldCount) + " +" + itoa(newStart) + "," + itoa(newCount) + " @@"
	out := []DiffLine{{Kind: "hunk", Text: header}}
	for _, op := range ops {
		out = append(out, DiffLine{Kind: op.kind, Text: op.text})
	}
	return out
}

func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	var digits [20]byte
	i := len(digits)
	for n > 0 {
		i--
		digits[i] = byte('0' + n%10)
		n /= 10
	}
	return string(digits[i:])
}
