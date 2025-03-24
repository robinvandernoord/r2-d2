use crate::cli::{Process, UploadOptions};
use crate::r2::R2D2;
use crate::r2_upload::upload_file;

impl Process for UploadOptions {
    async fn process(self) -> anyhow::Result<i32> {
        // subcommand 'upload':
        let r2 = R2D2::guess()?;
        // upload_file(r2, "/home/robin/Downloads/sport.vst".to_string(), None).await.unwrap_or_raise()?;
        upload_file(r2, "/home/robin/Downloads/praesides.png".to_string(), None).await?;

        Ok(0)
    }
}
