use serde::{Deserialize, Serialize};

/// One row of a unified diff. `kind` is hunk, same, del, or add.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    #[serde(rename = "kind")]
    pub kind: String,
    #[serde(rename = "text")]
    pub text: String,
}

fn dl(kind: &str, text: &str) -> DiffLine {
    DiffLine {
        kind: kind.to_string(),
        text: text.to_string(),
    }
}

/// A git-style unified diff of two texts, with three lines of context.
pub fn text_diff(before: &str, after: &str) -> Vec<DiffLine> {
    let old_lines = split_diff_lines(before);
    let new_lines = split_diff_lines(after);
    let ops = edit_ops(&old_lines, &new_lines);
    const CONTEXT: usize = 3;
    let changed: Vec<bool> = ops.iter().map(|op| op.kind != "same").collect();
    let mut keep = vec![false; ops.len()];
    for (i, _) in changed.iter().enumerate().filter(|(_, is_changed)| **is_changed) {
        let from = i.saturating_sub(CONTEXT);
        let mut to = i + CONTEXT;
        if to >= ops.len() {
            to = ops.len() - 1;
        }
        for k in keep.iter_mut().take(to + 1).skip(from) {
            *k = true;
        }
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < ops.len() {
        if !keep[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < ops.len() && keep[i] {
            i += 1;
        }
        out.extend(hunk(&ops[start..i]));
    }
    out
}

struct DiffOp<'a> {
    kind: &'static str,
    text: &'a str,
    old: usize,
    new: usize,
}

fn split_diff_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

fn edit_ops<'a>(old_lines: &[&'a str], new_lines: &[&'a str]) -> Vec<DiffOp<'a>> {
    let n = old_lines.len();
    let m = new_lines.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            if old_lines[i] == new_lines[j] {
                dp[i][j] = dp[i + 1][j + 1] + 1;
            } else if dp[i + 1][j] >= dp[i][j + 1] {
                dp[i][j] = dp[i + 1][j];
            } else {
                dp[i][j] = dp[i][j + 1];
            }
        }
    }
    let mut ops: Vec<DiffOp<'a>> = Vec::with_capacity(n + m);
    let mut old_line = 1usize;
    let mut new_line = 1usize;
    let mut add = |kind: &'static str, text: &'a str, ops: &mut Vec<DiffOp<'a>>| {
        ops.push(DiffOp {
            kind,
            text,
            old: old_line,
            new: new_line,
        });
        match kind {
            "same" => {
                old_line += 1;
                new_line += 1;
            }
            "del" => old_line += 1,
            "add" => new_line += 1,
            _ => {}
        }
    };
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old_lines[i] == new_lines[j] {
            add("same", old_lines[i], &mut ops);
            i += 1;
            j += 1;
            continue;
        }
        if dp[i + 1][j] >= dp[i][j + 1] {
            add("del", old_lines[i], &mut ops);
            i += 1;
            continue;
        }
        add("add", new_lines[j], &mut ops);
        j += 1;
    }
    while i < n {
        add("del", old_lines[i], &mut ops);
        i += 1;
    }
    while j < m {
        add("add", new_lines[j], &mut ops);
        j += 1;
    }
    ops
}

fn hunk(ops: &[DiffOp]) -> Vec<DiffLine> {
    if ops.is_empty() {
        return Vec::new();
    }
    let (mut old_start, mut new_start) = (ops[0].old, ops[0].new);
    let (mut old_count, mut new_count) = (0usize, 0usize);
    for op in ops {
        match op.kind {
            "same" => {
                old_count += 1;
                new_count += 1;
            }
            "del" => old_count += 1,
            "add" => new_count += 1,
            _ => {}
        }
    }
    if old_count == 0 {
        old_start = 0;
    }
    if new_count == 0 {
        new_start = 0;
    }
    let header = format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@");
    let mut out = vec![dl("hunk", &header)];
    for op in ops {
        out.push(dl(op.kind, op.text));
    }
    out
}
