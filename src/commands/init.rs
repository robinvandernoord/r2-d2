#![expect(dead_code, reason = "This file is a work in progress")]

use crate::cli::{InitOptions, Process};
use crate::commands::wipe::do_confirm_with_initial;
use crate::config::{prompt_account_config, prompt_bucket_config};
use crate::r2::{R2D2, ResticRepository};
use anyhow::bail;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use rustic_core::{BackupOptions, ConfigOptions, KeyOptions, PathList, SnapshotOptions};
use serde::Serialize;
use serde_json::Value;

/// Convert any `Serialize` struct into a `HeaderMap`
/// using Serde's default serialization rules.
pub fn to_headermap<T: Serialize>(value: &T) -> Result<HeaderMap, Box<dyn std::error::Error>> {
    let mut headers = HeaderMap::new();

    if let Ok(Value::Object(obj)) = serde_json::to_value(value) {
        for (key, raw_value) in obj {
            // Convert values into strings (strings stay as-is, others use `to_string`)
            let string_value = match raw_value {
                Value::String(s) => s,
                other => other.to_string(),
            };

            let name = HeaderName::from_bytes(key.as_bytes())?;
            let value = HeaderValue::from_str(&string_value)?;
            headers.insert(name, value);
        }
    }

    Ok(headers)
}

#[derive(Debug, Default, Serialize)]
pub enum BucketJurisdiction {
    #[default]
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "eu")]
    Europe,
    #[serde(rename = "fedramp")]
    Fedramp,
}

#[derive(Debug, Default, Serialize)]
pub struct CreateBucketHeaders {
    #[serde(rename = "cf-r2-jurisdiction")]
    pub cf_r2_jurisdiction: BucketJurisdiction,
}

#[derive(Debug, Default, Serialize)]
pub struct CreateBucketBody {
    pub name: String, // locationHint
                      // storageClass
}

#[derive(Debug, Default, Serialize)]
pub struct CreateBucketOptions {
    pub headers: Option<CreateBucketHeaders>,
    pub body: Option<CreateBucketBody>,
}

impl Into<HeaderMap> for CreateBucketHeaders {
    fn into(self) -> HeaderMap {
        to_headermap(&self).unwrap_or_default()
    }
}

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
    let first = name.chars().next().expect("Safe because length >= 3");
    let last = name.chars().last().expect("Safe because length >= 3");

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

pub fn prompt_bucket_name(default: Option<&str>) -> anyhow::Result<String> {
    let mut input = cliclack::Input::new(
        "Bucket name (Cloudflare R2 -> Buckets -> Name; it doesn't have to exist yet):",
    )
    .validate_interactively(validate_bucket_name);
    if let Some(d) = default {
        input = input.default_input(d);
    } else {
        input = input.placeholder("my-bucket");
    }
    let value: String = input.interact()?;
    Ok(value)
}

async fn ensure_bucket_ready(
    r2: &R2D2,
    yes: bool,
    no: bool,
) -> anyhow::Result<()> {
    let bucket = r2.bucket_or_default();
    if bucket.is_empty() || !r2.bucket_exists(None).await? {
        if yes || no {
            bail!("Bucket does not exist and non-interactive mode is enabled.");
        }
        if do_confirm_with_initial("Bucket not found. Create it now?", false, false, true) {
            r2.create_bucket_py(&bucket, Default::default()).await?;
        }
    }
    Ok(())
}

async fn get_r2_config(
    yes: bool,
    no: bool,
    cli_bucket: Option<String>,
) -> anyhow::Result<R2D2> {
    let mut r2 = R2D2::guess()?;

    // Bucket name is asked at most once here, only if still missing and CLI not provided
    if r2.bucket.is_none() {
        if let Some(bucket) = cli_bucket {
            r2.set_bucket(Some(bucket));
        } else {
            if yes || no {
                bail!("No bucket specified in config files or --bucket, exiting");
            }
            let bucket = prompt_bucket_name(None)?;
            r2.set_bucket(Some(bucket));
        }
    } else if cli_bucket.is_some() {
        r2.set_bucket(cli_bucket);
    }

    ensure_bucket_ready(&r2, yes, no).await?;
    Ok(r2)
}

async fn init_r2_config(cli_bucket: Option<String>) -> anyhow::Result<R2D2> {
    // Account section (collect + save)
    prompt_account_config().await?;

    // Bucket section (collect + save; returns the chosen bucket)
    let chosen_bucket = prompt_bucket_config(cli_bucket).await?;

    // Build R2D2 from fresh config and set bucket once
    let mut r2 = R2D2::guess()?;
    r2.set_bucket(Some(chosen_bucket));

    // Final existence/create step
    ensure_bucket_ready(&r2, false, false).await?;

    Ok(r2)
}

async fn get_or_init_r2_config(
    yes: bool,
    no: bool,
    cli_bucket: Option<String>,
) -> anyhow::Result<R2D2> {
    // Try to load existing config first

    if let Ok(r2) = get_r2_config(yes, no, cli_bucket.clone()).await {
        Ok(r2)
    } else {
        // No config found -> interactive bootstrap
        if yes || no {
            bail!(
                "No R2 configuration found and running non-interactively. Re-run without --yes/--no to configure interactively."
            );
        }

        init_r2_config(cli_bucket).await
    }
}

impl Process for InitOptions {
    async fn process(self) -> anyhow::Result<i32> {
        // Single clean interface to obtain configured R2D2, including bucket handling
        let r2 = get_or_init_r2_config(self.yes, self.no, self.bucket.clone()).await?;

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
