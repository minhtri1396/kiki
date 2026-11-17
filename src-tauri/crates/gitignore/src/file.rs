//! Writing the `.gitignore` text.

use anyhow::Result;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

/// The text of `raw` with `patterns` appended under `header` (added once). Line endings follow the file.
pub(crate) fn append_patterns(raw: &str, header: &str, patterns: &[String]) -> String {
    let crlf = raw.contains("\r\n");
    let body = raw.replace("\r\n", "\n");
    let body = body.strip_suffix('\n').unwrap_or(&body);
    let need_header = !body.split('\n').any(|line| line.trim() == header);
    let mut out = String::new();
    if !body.is_empty() {
        out.push_str(body);
        out.push('\n');
        if need_header {
            out.push('\n');
        }
    }
    if need_header {
        out.push_str(header);
        out.push('\n');
    }
    for pattern in patterns {
        out.push_str(pattern);
        out.push('\n');
    }
    if crlf {
        out = out.replace('\n', "\r\n");
    }
    out
}

/// Replaces the file through a temporary copy, keeping its permissions (0644 when new).
pub(crate) fn write_file(path: &str, text: &str) -> Result<()> {
    let mode = std::fs::metadata(path)
        .map(|info| info.permissions().mode() & 0o777)
        .unwrap_or(0o644);
    let tmp = format!("{path}.tmp");
    write_with_mode(&tmp, text.as_bytes(), mode)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

/// Creates or truncates the file; the mode applies only when it is created and is subject to the umask.
fn write_with_mode(path: &str, data: &[u8], mode: u32) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(path)?;
    f.write_all(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_added_once() {
        let first = append_patterns("keep\n", "# H", &["/a/".to_string()]);
        assert_eq!(first, "keep\n\n# H\n/a/\n");
        let second = append_patterns(&first, "# H", &["/b/".to_string()]);
        assert_eq!(second, "keep\n\n# H\n/a/\n/b/\n");
        assert_eq!(append_patterns("", "# H", &["/a".to_string()]), "# H\n/a\n");
    }
}
