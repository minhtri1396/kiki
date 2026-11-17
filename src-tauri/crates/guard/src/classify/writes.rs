//! Whether a shell command changes files: output redirects and the programs that write.
//! A program that is not known to be read-only counts as a writer.

use super::git::git_writes;
use super::tokens::{segment_command, split_command, tokenize};

/// True when the command redirects output to a file or runs a program that may change files.
pub(super) fn command_writes(command: &str) -> bool {
    if command.trim().is_empty() {
        return false;
    }
    if has_file_redirect(command) {
        return true;
    }
    for segment in split_command(command) {
        if segment_writes(&tokenize(&segment)) {
            return true;
        }
    }
    false
}

fn has_file_redirect(command: &str) -> bool {
    let bytes = command.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] != b'>' {
            continue;
        }
        if i + 1 < bytes.len() && bytes[i + 1] == b'&' {
            continue;
        }
        return true;
    }
    false
}

fn segment_writes(tokens: &[String]) -> bool {
    let (base, rest) = segment_command(tokens);
    match base.as_str() {
        "" | "gh" => false,
        "rtk" => segment_writes(rest),
        "git" => git_writes(rest),
        "sed" => sed_writes(rest),
        "find" => find_writes(rest),
        "codegraph" => codegraph_writes(rest),
        other => !read_only_command(other),
    }
}

fn read_only_command(name: &str) -> bool {
    matches!(
        name,
        "ls" | "cat"
            | "head"
            | "tail"
            | "less"
            | "more"
            | "wc"
            | "file"
            | "stat"
            | "pwd"
            | "whoami"
            | "id"
            | "date"
            | "uname"
            | "which"
            | "basename"
            | "dirname"
            | "realpath"
            | "readlink"
            | "diff"
            | "cmp"
            | "comm"
            | "sort"
            | "uniq"
            | "cut"
            | "tr"
            | "nl"
            | "od"
            | "hexdump"
            | "strings"
            | "du"
            | "tree"
            | "bat"
            | "rg"
            | "grep"
            | "egrep"
            | "fgrep"
            | "ag"
            | "awk"
            | "echo"
            | "printf"
            | "true"
            | "false"
            | "test"
            | "["
            | ":"
            | "type"
            | "column"
            | "fold"
            | "tac"
            | "rev"
            | "shasum"
            | "sha256sum"
            | "md5"
            | "md5sum"
            | "cksum"
            | "expr"
            | "seq"
            | "printenv"
    )
}

fn sed_writes(args: &[String]) -> bool {
    for arg in args {
        if arg == "--in-place" || arg.starts_with("--in-place=") {
            return true;
        }
        if arg.starts_with('-') && !arg.starts_with("--") && arg.trim_start_matches('-').contains('i') {
            return true;
        }
    }
    false
}

fn find_writes(args: &[String]) -> bool {
    args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "-delete" | "-exec" | "-execdir" | "-ok" | "-okdir" | "-fls" | "-fprint" | "-fprintf" | "-fprint0"
        )
    })
}

fn codegraph_writes(args: &[String]) -> bool {
    for arg in args {
        if arg.starts_with('-') {
            continue;
        }
        return !matches!(
            arg.as_str(),
            "explore"
                | "query"
                | "status"
                | "callers"
                | "callees"
                | "impact"
                | "affected"
                | "node"
                | "files"
                | "search"
                | "help"
                | "version"
        );
    }
    false
}
