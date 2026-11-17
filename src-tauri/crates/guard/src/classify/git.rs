//! Whether a `git` command changes the repository.

/// True unless the git subcommand only reads.
pub(super) fn git_writes(args: &[String]) -> bool {
    let (sub, rest) = git_subcommand(args);
    match sub.as_str() {
        "" | "status" | "log" | "diff" | "show" | "blame" | "rev-parse" | "ls-files" | "ls-tree" | "rev-list"
        | "describe" | "name-rev" | "shortlog" | "cat-file" | "grep" | "check-ignore" | "merge-base" | "help"
        | "version" | "var" => false,
        "branch" => git_branch_writes(rest),
        "remote" => git_remote_writes(rest),
        "config" => git_config_writes(rest),
        "stash" => git_stash_writes(rest),
        "tag" => git_tag_writes(rest),
        _ => true,
    }
}

fn git_subcommand(args: &[String]) -> (String, &[String]) {
    let mut i = 0;
    while i < args.len() {
        let tok = &args[i];
        if tok == "--" {
            if i + 1 < args.len() {
                return (args[i + 1].clone(), &args[i + 2..]);
            }
            return (String::new(), &[]);
        }
        if tok.starts_with('-') {
            if git_option_takes_arg(tok) && !tok.contains('=') {
                i += 1;
            }
            i += 1;
            continue;
        }
        return (tok.clone(), &args[i + 1..]);
    }
    (String::new(), &[])
}

fn git_option_takes_arg(tok: &str) -> bool {
    matches!(
        tok,
        "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" | "--super-prefix" | "--config-env"
    )
}

fn git_branch_writes(args: &[String]) -> bool {
    for arg in args {
        if arg == "--" {
            continue;
        }
        if arg.starts_with('-') {
            if matches!(
                arg.as_str(),
                "-d" | "-D"
                    | "-m"
                    | "-M"
                    | "-c"
                    | "-C"
                    | "-u"
                    | "--delete"
                    | "--move"
                    | "--copy"
                    | "--force"
                    | "--edit-description"
                    | "--unset-upstream"
            ) {
                return true;
            }
            if arg.starts_with("--set-upstream-to") || arg.starts_with("--delete") {
                return true;
            }
            continue;
        }
        return true;
    }
    false
}

fn git_remote_writes(args: &[String]) -> bool {
    let mut sub = "";
    for arg in args {
        if arg.starts_with('-') {
            continue;
        }
        sub = arg;
        break;
    }
    !matches!(sub, "" | "show" | "get-url")
}

fn git_config_writes(args: &[String]) -> bool {
    let mut positional = 0;
    for arg in args {
        if matches!(
            arg.as_str(),
            "--get" | "--list" | "-l" | "--get-regexp" | "--get-all" | "--get-urlmatch"
        ) {
            return false;
        }
        if !arg.starts_with('-') {
            positional += 1;
        }
    }
    positional > 0
}

fn git_stash_writes(args: &[String]) -> bool {
    for arg in args {
        if arg.starts_with('-') {
            continue;
        }
        return arg != "list" && arg != "show";
    }
    true
}

fn git_tag_writes(args: &[String]) -> bool {
    for arg in args {
        if arg == "-l" || arg == "--list" || arg.starts_with("--list=") {
            return false;
        }
    }
    for arg in args {
        if !arg.starts_with('-') {
            return true;
        }
    }
    false
}
