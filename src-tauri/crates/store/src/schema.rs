//! Creates and upgrades every table of `kiki.db`, in the order the migrations need.

use anyhow::Result;
use rusqlite::Connection;

use crate::groups::GroupRepo;
use crate::launches::LaunchRepo;
use crate::runs::RunRepo;
use crate::tasks;

/// Brings an opened database up to date. `setup_path` is the setup file that older versions kept the
/// launch list in.
pub(crate) fn init(db: &Connection, setup_path: &str) -> Result<()> {
    LaunchRepo::init(db)?;
    LaunchRepo::migrate_from_setup(db, setup_path)?;
    tasks::schema::init(db)?;
    GroupRepo::init(db)?;
    GroupRepo::new(db).migrate_task_groups()?;
    RunRepo::init(db)?;
    tasks::schema::migrate_ids(db)
}
