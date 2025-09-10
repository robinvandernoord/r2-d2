#![expect(dead_code, reason = "This file is a work in progress")]

use crate::cli::{InitOptions, Process};
use crate::commands::wipe::do_confirm_with_initial;
use crate::r2::{R2D2, ResticRepository};
use anyhow::bail;
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

#[expect(
    clippy::ptr_arg,
    reason = "compatibility with clicklack validate_interactively"
)]
fn validate_bucket_name(name: &String) -> Result<(), &'static str> {
    let error_msg = "Cloudflare R2 bucket names must be 3-63 characters long, can only contain lowercase letters, numbers (0-9), and hyphens (-), and cannot start or end with a hyphen.";

    if name.len() < 3 || name.len() > 63 {
        return Err(error_msg);
    }

    // Safe because length >= 3
    let first = name.chars().next().unwrap();
    let last = name.chars().last().unwrap();

    if !(first.is_ascii_lowercase() || first.is_ascii_digit())
        || !(last.is_ascii_lowercase() || last.is_ascii_digit())
    {
        return Err(error_msg);
    }

    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(error_msg);
    }

    Ok(())
}

impl Process for InitOptions {
    async fn process(self) -> anyhow::Result<i32> {
        let Ok(mut r2) = R2D2::guess() else {
            // todo: allow specify bucket
            //       -> should be able to start with an empty config and just ask everything (unless --yes/--no)

            todo!("interactive config")
        };

        r2.set_bucket(self.bucket);

        if r2.bucket.is_none() {
            if self.yes || self.no {
                // non-interactive -> bye
                bail!("No bucket specified in config files or --bucket, exiting")
            }

            let bucket: String = cliclack::Input::new("Bucket name:")
                .placeholder("my-bucket")
                .validate_interactively(validate_bucket_name)
                .interact()?;

            r2.set_bucket(Some(bucket));
        }

        // Check if bucket exists, or create it:
        if !r2.bucket_exists(None).await? {
            if do_confirm_with_initial(
                "Would you like to create the bucket?",
                self.yes,
                self.no,
                true,
            ) {
                todo!("create bucket");
            } else {
                return Ok(1);
            }
        }

        let repo = r2.into_rustic()?;

        // Init repository
        init_repo(repo)?;

        // Make snapshot
        // create_snapshot(repo.clone())?;

        // List snapshots
        // get_snapshots(repo)?;

        // Test Progressbar
        // crate::rustic_progress::test_progressbar();

        Ok(0)
    }
}
