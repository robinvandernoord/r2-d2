#![expect(dead_code, reason = "This file is a work in progress")]

use crate::cli::{InitOptions, Process};
use crate::commands::wipe::do_confirm_with_initial;
use crate::r2::{R2D2, ResticRepository, read_configfile};
use anyhow::bail;
use resolve_path::PathResolveExt;
use rustic_core::{BackupOptions, ConfigOptions, KeyOptions, PathList, SnapshotOptions};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tokio::fs;

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

fn should_use_export(path: &Path) -> bool {
    path.file_name().unwrap_or_default() == ".r2"
}

#[derive(Clone, Copy)]
struct MaskOptions {
    show_head: usize,
    show_tail: usize,
    mask: &'static str,
}

impl Default for MaskOptions {
    fn default() -> Self {
        Self {
            show_head: 4,
            show_tail: 4,
            mask: "***",
        }
    }
}

fn mask_secret_with_options(
    existing: &str,
    opts: MaskOptions,
) -> String {
    if existing.is_empty() {
        return String::new();
    }
    let len = existing.chars().count();
    if len <= opts.show_head + opts.show_tail {
        return opts.mask.to_string();
    }
    let head: String = existing.chars().take(opts.show_head).collect();
    let tail: String = existing
        .chars()
        .rev()
        .take(opts.show_tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}{}{}", head, opts.mask, tail)
}

// Config destinations for account/bucket files
#[derive(Clone, Copy)]
enum ConfigLocation {
    HomeConfigR2, // ~/.config/.r2
    HomeR2,       // ~/.r2
    CwdR2,        // ./.r2
    CwdEnv,       // ./.env
}

impl ConfigLocation {
    const fn value(self) -> &'static str {
        match self {
            Self::HomeConfigR2 => "home_config_r2",
            Self::HomeR2 => "home_r2",
            Self::CwdR2 => "cwd_r2",
            Self::CwdEnv => "cwd_env",
        }
    }
    const fn label(self) -> &'static str {
        match self {
            Self::HomeConfigR2 => "~/.config/.r2",
            Self::HomeR2 => "~/.r2",
            Self::CwdR2 => "./.r2",
            Self::CwdEnv => "./.env",
        }
    }
    const fn hint(self) -> &'static str {
        match self {
            Self::HomeConfigR2 => "config for all projects, recommended for account info",
            Self::HomeR2 => "user-level config",
            Self::CwdR2 => "project-level config, recommended for bucket info",
            Self::CwdEnv => ".env format (no export)",
        }
    }
    fn to_pathbuf(self) -> PathBuf {
        Path::new(self.label()).resolve().into()
    }

    const fn as_clicklack(&self) -> (&str, &str, &str) {
        (self.value(), self.label(), self.hint())
    }

    fn from_value(val: &str) -> Self {
        match val {
            "home_config_r2" => Self::HomeConfigR2,
            "home_r2" => Self::HomeR2,
            "cwd_r2" => Self::CwdR2,
            "cwd_env" => Self::CwdEnv,
            _ => {
                eprintln!("Unexpected value '{val}', returning default (~/.config/.r2) instead.");
                Self::HomeConfigR2
            },
        }
    }
}

// Helper: quote only when needed; prefer double quotes; escape backslash, double quote, dollar
fn format_line(
    key: &str,
    value: &str,
    line_use_export: bool,
) -> String {
    let needs_quotes =
        value.chars().any(char::is_whitespace) || value.contains(['#', '"', '\'', '$', '\\', '=']);
    let val = if needs_quotes {
        let escaped = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$");
        format!("\"{escaped}\"")
    } else {
        value.to_string()
    };
    if line_use_export {
        format!("export {key}={val}")
    } else {
        format!("{key}={val}")
    }
}

async fn write_env_file(
    path: &Path,
    vars: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    // Read existing file (preserve comments and unknown lines)
    let existing_text = fs::read_to_string(path).await.unwrap_or_default();
    let mut lines: Vec<String> = if existing_text.is_empty() {
        Vec::new()
    } else {
        existing_text.lines().map(ToString::to_string).collect()
    };

    // Decide export style:
    // - If file already contains any `export KEY=...`, keep using export
    // - Otherwise, fall back to filename heuristic
    let file_uses_export = lines.iter().any(|l| {
        let t = l.trim();
        !t.starts_with('#') && t.starts_with("export ") && t.contains('=')
    });
    let use_export = if lines.is_empty() {
        should_use_export(path)
    } else {
        file_uses_export
    };

    // Build index of existing keys -> (line_idx, had_export_on_that_line)
    let mut key_location: std::collections::HashMap<String, (usize, bool)> =
        std::collections::HashMap::new();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed_line = line.trim_start();
        if trimmed_line.is_empty() || trimmed_line.starts_with('#') {
            continue;
        }

        let (had_export, rest) = trimmed_line
            .strip_prefix("export ")
            .map_or((false, trimmed_line), |rest| (true, rest));

        if let Some(eq) = rest.find('=') {
            let key = rest[..eq].trim().to_string();
            if !key.is_empty() {
                key_location.insert(key, (idx, had_export));
            }
        }
    }

    // Header management: ensure "Generated by R2-D2 interactive init" appears at most once.
    // - If file is empty, start with the header.
    // - If file is non-empty and header missing, append it at the end to avoid disturbing existing comments.
    let header = "# Generated by R2-D2 interactive init";
    let has_header = lines.iter().any(|l| l.trim() == header);
    if lines.is_empty() {
        lines.push(header.to_string());
    } else if !has_header {
        // Ensure previous content ends with a newline visually when re-joined
        if let Some(last) = lines.last()
            && !last.is_empty()
        {
            lines.push(String::new());
        }
        lines.push(header.to_string());
    }

    // Apply updates: replace in place when key exists; otherwise append
    for (key, value) in vars {
        if let Some((idx, had_export)) = key_location.get(key).copied() {
            // Respect the original line's export usage
            lines[idx] = format_line(key, value, had_export);
        } else {
            // Append using the file's export style
            lines.push(format_line(key, value, use_export));
        }
    }

    // Join and write back
    let mut out = String::new();
    for line in &lines {
        out.push_str(line);
        out.push('\n');
    }

    fs::write(path, out).await?;
    Ok(())
}

fn prompt_bucket_name(default: Option<&str>) -> anyhow::Result<String> {
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

async fn prompt_account_config() -> anyhow::Result<()> {
    // Choose where to store account config
    let account_choices = [
        ConfigLocation::HomeConfigR2,
        ConfigLocation::HomeR2,
        ConfigLocation::CwdR2,
        ConfigLocation::CwdEnv,
    ];
    let account_items: Vec<_> = account_choices
        .iter()
        .map(ConfigLocation::as_clicklack)
        .collect();

    let selected_account_value =
        cliclack::Select::new("Where would you like to store your ACCOUNT config?")
            .items(&account_items)
            .initial_value(ConfigLocation::HomeConfigR2.value())
            .interact()?;

    let account_loc = ConfigLocation::from_value(selected_account_value);
    let account_path = account_loc.to_pathbuf();

    // Load existing for defaults
    let existing = read_configfile(&account_path).unwrap_or_default();

    // Collect fields
    let mut account_vars = BTreeMap::new();

    {
        let mut input = cliclack::Input::new(
            "Cloudflare R2 Account ID (Dashboard: R2 -> Overview -> Account ID):",
        );
        if let Some(v) = existing.get("R2_ACCOUNT_ID") {
            input = input.default_input(v);
        } else {
            input = input.placeholder("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
        }
        let val: String = input.interact()?;
        account_vars.insert("R2_ACCOUNT_ID".to_string(), val);
    }

    {
        let masked_default = existing
            .get("R2_API_TOKEN")
            .map(|s| mask_secret_with_options(s, MaskOptions::default()))
            .unwrap_or_default();

        let mut input = cliclack::input(
            "Cloudflare API Token (Profile -> API Tokens; leave as masked or empty to keep existing):",
        );
        if masked_default.is_empty() {
            input = input.placeholder("your-api-token");
        } else {
            input = input.default_input(&masked_default);
        }
        let entered: String = input.interact()?;

        let val: String =
            if !masked_default.is_empty() && (entered.is_empty() || entered == masked_default) {
                existing.get("R2_API_TOKEN").cloned().unwrap_or_default()
            } else {
                entered
            };
        account_vars.insert("R2_API_TOKEN".to_string(), val);
    }

    // Save once
    write_env_file(&account_path, &account_vars).await?;
    Ok(())
}

async fn prompt_bucket_config(cli_bucket: Option<String>) -> anyhow::Result<String> {
    // Choose where to store bucket config
    let bucket_choices = [
        ConfigLocation::CwdR2,
        ConfigLocation::HomeR2,
        ConfigLocation::HomeConfigR2,
        ConfigLocation::CwdEnv,
    ];

    let bucket_items: Vec<_> = bucket_choices
        .iter()
        .map(ConfigLocation::as_clicklack)
        .collect();

    let selected_bucket_value =
        cliclack::select("Where would you like to store your BUCKET config?")
            .items(&bucket_items)
            .initial_value(ConfigLocation::CwdR2.value())
            .interact()?;

    let bucket_loc = ConfigLocation::from_value(selected_bucket_value);
    let bucket_path = bucket_loc.to_pathbuf();

    // Load existing for defaults
    let existing = read_configfile(&bucket_path).unwrap_or_default();

    // Ask BUCKET NAME exactly once (CLI overrides)
    let chosen_bucket = if let Some(bucket) = cli_bucket {
        bucket
    } else {
        prompt_bucket_name(existing.get("R2_BUCKET").map(String::as_str))?
    };

    // Keys missing?
    let keys_missing = existing
        .get("R2_ACCESS_KEY_ID")
        .is_none_or(String::is_empty)
        || existing
            .get("R2_SECRET_ACCESS_KEY")
            .is_none_or(String::is_empty);

    if keys_missing {
        let want_auto_tokens = cliclack::confirm(
            "No R2 access keys found. Create bucket-scoped access keys automatically using your API token?",
        )
            .initial_value(true)
            .interact()?;
        if want_auto_tokens {
            todo!("automatically create bucket-scoped access keys using the provided API token");
        }
    }

    // Collect keys (with defaults)
    let mut bucket_vars = BTreeMap::new();

    {
        let mut input = cliclack::Input::new(
            "R2 Access Key ID (Dashboard: R2 -> S3 API -> Create API Token -> Access Key ID):",
        );
        if let Some(v) = existing.get("R2_ACCESS_KEY_ID") {
            input = input.default_input(v);
        } else {
            input = input.placeholder("your-access-key-id");
        }
        let val: String = input.interact()?;
        bucket_vars.insert("R2_ACCESS_KEY_ID".to_string(), val);
    }

    {
        let masked_default = existing
            .get("R2_SECRET_ACCESS_KEY")
            .map(|s| mask_secret_with_options(s, MaskOptions::default()))
            .unwrap_or_default();

        let mut input =
            cliclack::Input::new("R2 Secret Access Key (shown once when creating the API token):");

        if masked_default.is_empty() {
            input = input.placeholder("your-secret-access-key");
        } else {
            input = input.default_input(&masked_default);
        }
        let entered: String = input.interact()?;

        let val: String =
            if !masked_default.is_empty() && (entered.is_empty() || entered == masked_default) {
                existing
                    .get("R2_SECRET_ACCESS_KEY")
                    .cloned()
                    .unwrap_or_default()
            } else {
                entered
            };
        bucket_vars.insert("R2_SECRET_ACCESS_KEY".to_string(), val);
    }

    // Bucket last
    bucket_vars.insert("R2_BUCKET".to_string(), chosen_bucket.clone());

    // Save once (writer preserves comments and appends new keys; order is enforced by its logic)
    write_env_file(&bucket_path, &bucket_vars).await?;

    Ok(chosen_bucket)
}

async fn ensure_bucket_ready(
    r2: &R2D2,
    yes: bool,
    no: bool,
) -> anyhow::Result<()> {
    if !r2.bucket_exists(None).await? {
        if yes || no {
            bail!("Bucket does not exist and non-interactive mode is enabled.");
        }
        if do_confirm_with_initial("Bucket not found. Create it now?", false, false, true) {
            todo!("create bucket");
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
