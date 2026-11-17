//! Holds the KIKI agent instruction documents. The markdown files are compiled into the binary.
//!
//! Only the `requirement/` and `development/` trees are embedded, so nothing else in the crate directory
//! (sources, manifest) is reachable through [`file`].

use include_dir::{include_dir, Dir};

static REQUIREMENT: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/requirement");
static DEVELOPMENT: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/development");

/// The content of an embedded agent document. Paths start with the tree name, e.g.
/// `requirement/planner.md` and `development/backend/runner.md`. `None` for anything else.
pub fn file(path: &str) -> Option<&'static [u8]> {
    let (tree, rest) = path.split_once('/')?;
    let dir = match tree {
        "requirement" => &REQUIREMENT,
        "development" => &DEVELOPMENT,
        _ => return None,
    };
    dir.get_file(rest).map(|f| f.contents())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_only_the_two_markdown_trees() {
        let planner = file("requirement/planner.md").expect("planner");
        assert!(!planner.is_empty());
        assert!(file("development/backend/runner.md").is_some());
        assert!(file("Cargo.toml").is_none());
        assert!(file("src/lib.rs").is_none());
        assert!(file("requirement/../Cargo.toml").is_none());
        assert!(file("requirement/missing.md").is_none());
        assert!(file("requirement").is_none());
    }
}
