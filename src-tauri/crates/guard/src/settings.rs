//! Reading and writing the JSON settings files that hold AI tool hooks. These are the steps the install flow
//! (`HookDialect::install`) is built from: read the file, change our entries, write it back atomically.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use anyhow::bail;
use kiki_util::json::marshal_indent;
use serde_json::{Map, Value};

/// A settings file as a JSON object.
pub(crate) type Root = Map<String, Value>;

/// Reads a file; a file that does not exist is `None`.
pub(crate) fn read_optional(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Parses settings text: `null` is an empty object, any other value that is not an object is an error.
pub(crate) fn parse_root(raw: &[u8]) -> anyhow::Result<Root> {
    match serde_json::from_slice::<Value>(raw)? {
        Value::Object(m) => Ok(m),
        Value::Null => Ok(Map::new()),
        other => {
            let kind = match other {
                Value::Array(_) => "array",
                Value::String(_) => "string",
                Value::Bool(_) => "bool",
                _ => "number",
            };
            bail!("json: cannot unmarshal {kind} into Go value of type map[string]interface {{}}")
        }
    }
}

pub(crate) fn is_blank(raw: &[u8]) -> bool {
    String::from_utf8_lossy(raw).trim().is_empty()
}

/// The settings object at `path`; a missing or blank file is an empty object.
pub(crate) fn read_root(path: &Path) -> anyhow::Result<Root> {
    match read_optional(path)? {
        Some(raw) if !is_blank(&raw) => parse_root(&raw),
        _ => Ok(Map::new()),
    }
}

/// Removes `root["hooks"]`; a missing or null value is `None`, anything but an object is an error.
pub(crate) fn take_hooks(root: &mut Root) -> anyhow::Result<Option<Map<String, Value>>> {
    match root.remove("hooks") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(m)) => Ok(Some(m)),
        Some(_) => bail!("hooks are not an object"),
    }
}

/// Writes the settings object, indented and HTML-safe like the files already on disk.
pub(crate) fn write_root(path: &Path, root: Root) -> anyhow::Result<()> {
    write_atomic(path, marshal_indent(&Value::Object(root)).as_bytes())
}

/// Writes `data` (trimmed, one trailing newline) to a temporary file and renames it over `path`,
/// creating missing parent folders.
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::DirBuilder::new().recursive(true).mode(0o755).create(parent)?;
    }
    let mut body = String::from_utf8_lossy(data).trim().as_bytes().to_vec();
    body.push(b'\n');
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o644)
        .open(&tmp)?;
    file.write_all(&body)?;
    drop(file);
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{join, p, tmp};

    #[test]
    fn parse_root_accepts_objects_and_null_only() {
        assert!(parse_root(b"{\"a\":1}").unwrap().contains_key("a"));
        assert!(parse_root(b"null").unwrap().is_empty());
        let err = parse_root(b"[1]").unwrap_err();
        assert_eq!(
            err.to_string(),
            "json: cannot unmarshal array into Go value of type map[string]interface {}"
        );
        assert!(parse_root(b"{").is_err());
    }

    #[test]
    fn read_root_treats_missing_and_blank_files_as_empty() {
        let dir_dir = tmp();
        let dir = p(&dir_dir);
        assert!(read_root(Path::new(&join(&dir, "none.json"))).unwrap().is_empty());
        let blank = join(&dir, "blank.json");
        fs::write(&blank, " \n").unwrap();
        assert!(read_root(Path::new(&blank)).unwrap().is_empty());
    }

    #[test]
    fn take_hooks_rejects_a_non_object() {
        let mut root = parse_root(b"{\"hooks\":[]}").unwrap();
        assert_eq!(
            take_hooks(&mut root).unwrap_err().to_string(),
            "hooks are not an object"
        );
        let mut root = parse_root(b"{\"hooks\":null}").unwrap();
        assert!(take_hooks(&mut root).unwrap().is_none());
    }

    #[test]
    fn write_atomic_creates_folders_trims_and_leaves_no_temp_file() {
        let dir_dir = tmp();
        let dir = p(&dir_dir);
        let path = join(&dir, "a/b/hooks.json");
        write_atomic(Path::new(&path), b"  {}  \n\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}\n");
        assert!(!Path::new(&format!("{path}.tmp")).exists());
    }
}
