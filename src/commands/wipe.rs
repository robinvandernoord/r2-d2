use crate::cli::{Process, WipeOptions};
use crate::r2::R2D2;
use crate::r2_purge::empty_repo;
use anyhow::bail;
use cliclack;
use std::fmt::Display;

#[derive(Debug, Default)]
pub struct DeleteOptions {}

pub fn confirm<S: Display>(
    prompt: S,
    initial: bool,
) -> bool {
    let mut input_confirmation = cliclack::confirm(prompt).initial_value(initial);

    input_confirmation.interact().unwrap_or_default()
}

pub fn do_confirm<S: Display>(
    prompt: S,
    yes: bool,
    no: bool,
) -> bool {
    !no && (yes || confirm(prompt, false))
}

pub fn do_confirm_with_initial<S: Display>(
    prompt: S,
    yes: bool,
    no: bool,
    initial: bool,
) -> bool {
    !no && (yes || confirm(prompt, initial))
}

impl Process for WipeOptions {
    async fn process(self) -> anyhow::Result<i32> {
        let mut r2 = R2D2::guess()?;

        if self.bucket.is_some() {
            r2.set_bucket(self.bucket);
        }

        let Some(bucket) = &r2.bucket else {
            bail!("No bucket configured to wipe!")
        };

        if do_confirm(
            format!("Are you sure you want to remove bucket '{}'? [yN]", &bucket),
            self.yes,
            false,
        ) {
            if self.include_contents {
                empty_repo(&r2).await?;
            }

            if self.include_bucket {
                r2.delete_bucket_py(bucket, None).await?;

                eprintln!("Bucket `{bucket}` deleted.");
            }
        }

        Ok(0)
    }
}
