//! Team folder sharing: save this user's agents and repositories into a shared folder, and pull a teammate's.
//!
//! Pattern: Controller + Facade. [`TeamController`] maps requests to the service methods on `Server`
//! (`service`); `status` answers the state payload, `views` and `diff` compare trees, `import` pulls,
//! `selected` copies the agents the user ticked, `shared_repo` is the shared repository file format,
//! `picker` is the injected folder dialog.

mod controller;
mod diff;
mod files;
mod import;
mod picker;
mod selected;
mod service;
mod shared_repo;
mod status;
mod views;

#[cfg(test)]
mod tests;

pub(crate) use controller::TeamController;
pub use picker::FolderPicker;

/// The answer when a team operation needs a saved folder and user first.
const SETUP_FIRST: &str = "Set up the team folder in Settings first.";

/// How a save or copy ended: done, or the user must decide something first (answered with 409).
#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    Done,
    Conflict(serde_json::Value),
}

/// Which way files are copied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    /// From the team folder into this app.
    Pull,
    /// From this app into the team folder.
    Push,
}

impl Direction {
    fn parse(raw: &str) -> Option<Direction> {
        match raw {
            "pull" => Some(Direction::Pull),
            "push" => Some(Direction::Push),
            _ => None,
        }
    }
}

/// What is compared or copied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Agents,
    Repos,
}

impl Part {
    /// An empty value is `default`; anything but `agents` and `repos` is `None`.
    fn parse(raw: &str, default: Part) -> Option<Part> {
        match raw {
            "" => Some(default),
            "agents" => Some(Part::Agents),
            "repos" => Some(Part::Repos),
            _ => None,
        }
    }
}
