use anyhow::{Context, bail};
use std::path::Path;

use opendal::Operator;
use rustic_core::Progress;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

use crate::r2::R2D2;
use crate::rustic_progress::ProgressBar;

// In bytes; minimum chunk size is 5 MB; increase CHUNK_SIZE to send larger chunks:
const CHUNK_SIZE: u64 = 1024 * 1024 * 50; // 50 MB
const MAX_CHUNKS: u64 = 10000;

async fn upload_file_with_opendal(
    operator: Operator,
    file_path: &str,
) -> anyhow::Result<String> {
    let path = Path::new(file_path);

    let key = path
        .file_name()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
        .to_owned();

    let metadata = tokio::fs::metadata(path)
        .await
        .with_context(|| format!("`{file_path}` does not seem to exist."))?;
    let file_size = metadata.len();

    if file_size == 0 {
        bail!("Bad file size (0).");
    }

    let mut chunk_count = (file_size / CHUNK_SIZE) + 1;
    let mut size_of_last_chunk = file_size % CHUNK_SIZE;
    if size_of_last_chunk == 0 {
        size_of_last_chunk = CHUNK_SIZE;
        chunk_count -= 1;
    }

    if chunk_count > MAX_CHUNKS {
        bail!("Too many chunks! Try increasing your chunk size.");
    }

    let pb = ProgressBar::bytes(key.clone());
    pb.set_title("Uploading");
    pb.set_length(file_size);

    let mut writer = operator.writer(&key).await?;

    // Read and upload in chunks
    let mut file = File::open(path).await?;

    for chunk_index in 0..chunk_count {
        let this_chunk_size = if chunk_index == chunk_count - 1 {
            size_of_last_chunk
        } else {
            CHUNK_SIZE
        };

        let mut buffer = vec![0u8; this_chunk_size as usize];
        file.read_exact(&mut buffer).await.with_context(|| {
            format!(
                "Failed to read chunk {} from `{}`",
                chunk_index + 1,
                file_path
            )
        })?;

        writer
            .write(buffer)
            .await
            .with_context(|| format!("Failed to upload chunk {}", chunk_index + 1))?;

        pb.inc(this_chunk_size);
    }

    writer.close().await?;
    pb.finish();

    Ok(key)
}
pub async fn upload_file(
    r2: &R2D2,
    file_path: String,
    bucket: Option<String>,
) -> anyhow::Result<String> {
    let mut r2_op = r2.clone();

    if bucket.is_some() {
        r2_op.set_bucket(bucket);
    }

    let bucket = r2_op.bucket.clone();

    let op = r2_op.into_opendal_backend()?.into_operator();
    let key = upload_file_with_opendal(op, &file_path).await?;

    // bucket domain + key = public url
    let url = r2
        .bucket_domain(bucket)
        .await
        .map_or_else(String::new, |domain| {
            domain
                .join(&key)
                .map(|it| it.to_string())
                .unwrap_or_default()
        });

    Ok(dbg!(url))
}
