use anyhow::bail;
use crate::cli::{Process, WipeOptions};
use crate::r2::R2D2;
use crate::r2_purge::empty_repo;

#[derive(Debug, Default)]
pub struct DeleteOptions {}

impl Process for WipeOptions {
    async fn process(self) -> anyhow::Result<i32> {
        if(!self.yes) {
            todo!("Still need to find a nice library to do confirmations etc.")
        }

        let mut r2 = R2D2::guess()?;

        if self.bucket.is_some() {
            r2.set_bucket(self.bucket)
        }

        let Some(bucket) = &r2.bucket else {
            bail!("No bucket configured to wipe!")
        };


        if self.include_contents {
            empty_repo(&r2).await?;
        }

        if self.include_bucket {
            r2.delete_bucket_py(&bucket, None).await?;

            eprintln!("Bucket `{bucket}` deleted.");
        }



        Ok(0)
    }
}