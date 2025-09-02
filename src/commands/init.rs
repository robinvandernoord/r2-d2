#![expect(dead_code, reason = "This file is a work in progress")]

use crate::cli::{InitOptions, Process};
use crate::r2::{R2D2, ResticRepository};
use rustic_core::{BackupOptions, ConfigOptions, KeyOptions, PathList, SnapshotOptions};

pub fn init_repo(repo: ResticRepository) -> anyhow::Result<()> {
    let key_opts = KeyOptions::default();
    let config_opts = ConfigOptions::default();
    repo.init(&key_opts, &config_opts)?;

    Ok(())
}

fn create_snapshot(repo: ResticRepository) -> anyhow::Result<()> {
    // Turn repository state to indexed (for backup):
    let repo = repo.open()?.to_indexed_ids()?;

    // Pre-define the snapshot-to-backup
    let snap = SnapshotOptions::default()
        .add_tags("tag1,tag2")?
        .to_snapshot()?;

    // Specify backup options and source
    let backup_opts = BackupOptions::default();

    // use - for stdin
    let source = PathList::from_string("src")?.sanitize()?;

    // run the backup and return the snapshot pointing to the backup'ed data.
    let snap = repo.backup(&backup_opts, &source, snap)?;

    dbg!(snap.time);

    Ok(())
}

fn get_snapshots(repo: ResticRepository) -> anyhow::Result<()> {
    let repo = repo.open()?;

    // Get all snapshots from the repository
    let snaps = repo.get_all_snapshots()?;

    dbg!(snaps.len());

    Ok(())
}

impl Process for InitOptions {
    async fn process(self) -> anyhow::Result<i32> {
        let r2 = R2D2::guess()?;

        let repo = r2.into_rustic()?;

        // Init repository
        // init_repo(repo)?;

        // Make snapshot
        // create_snapshot(repo.clone())?;

        // List snapshots
        get_snapshots(repo)?;

        // Test Progressbar
        // crate::rustic_progress::test_progressbar();

        Ok(0)
    }
}
