use anyhow::{Context, bail};
use std::future::Future;
use std::io;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_smithy_types::byte_stream::{ByteStream, Length};
use futures::future::try_join_all;
use scopeguard::defer;
use tokio::sync::Mutex;

use crate::r2::R2D2;

// In bytes; minimum chunk size is 5 MB; increase CHUNK_SIZE to send larger chunks:
const CHUNK_SIZE: u64 = 1024 * 1024 * 15;
const MAX_CHUNKS: u64 = 10000;

struct MultipartUpload<'a> {
    client: &'a S3Client,
    path: &'a Path,
    upload_id: &'a str,
    key: &'a str,
    bucket: &'a str,
}

impl<'a> MultipartUpload<'a> {
    pub const fn new(
        client: &'a S3Client,
        path: &'a Path,
        upload_id: &'a str,
        key: &'a str,
        bucket: &'a str,
    ) -> Self {
        Self {
            client,
            path,
            upload_id,
            key,
            bucket,
        }
    }
}

async fn upload_chunk(
    to_upload: &MultipartUpload<'_>,
    part_number: i32,
    chunk_index: u64,
    chunk_size: u64,
    progress: Arc<Mutex<ProgressState>>,
) -> anyhow::Result<CompletedPart> {
    let stream = ByteStream::read_from()
        .path(to_upload.path)
        .offset(chunk_index * CHUNK_SIZE)
        .length(Length::UpTo(chunk_size))
        .build()
        .await?;

    let upload_part_res = to_upload
        .client
        .upload_part()
        .key(to_upload.key)
        .bucket(to_upload.bucket)
        .upload_id(to_upload.upload_id)
        .body(stream)
        .part_number(part_number)
        .send()
        .await
        .with_context(|| format!("Something went wrong uploading part {part_number}."))?;

    // get mutex lock and update value:
    progress.lock().await.update(chunk_size);

    let e_tag = upload_part_res.e_tag.unwrap_or_default();

    Ok(CompletedPart::builder()
        .e_tag(e_tag)
        .part_number(part_number)
        .build())
}

#[derive(Debug)]
struct ProgressState {
    total_uploaded: f64,
    expected_size: f64,
}

#[allow(clippy::cast_precision_loss)]
impl ProgressState {
    const fn new(expected_size: u64) -> Self {
        Self {
            total_uploaded: 0f64,
            expected_size: expected_size as f64,
        }
    }

    fn update(
        &mut self,
        uploaded: u64,
    ) {
        self.total_uploaded += uploaded as f64;
    }

    #[allow(dead_code)]
    fn percentage(&self) -> String {
        let raw = self.total_uploaded / self.expected_size;

        format!("{:.2}%", raw * 100.0)
    }

    fn bar(&self) -> String {
        let raw = self.total_uploaded / self.expected_size;
        let percent = raw * 100.0;

        // Define the number of characters for the bar, e.g., 20 characters
        let bar_length: i32 = 20;

        // Calculate the number of '#' characters to display based on the percentage
        let filled_length = (f64::from(bar_length) * raw).round() as i32;

        // Create the progress bar string
        let mut bar = String::new();
        for _ in 0..filled_length {
            bar.push('#');
        }
        for _ in filled_length..bar_length {
            bar.push(' ');
        }

        // Format the output with percentage and progress bar
        format!(
            "[{}]{} {:.2}%",
            bar,
            if percent >= 100.0 {
                "##"
            } else if percent >= 95.0 {
                "# "
            } else {
                "  "
            },
            percent
        )
    }
}

async fn display_upload_feedback(progress: Arc<Mutex<ProgressState>>) {
    let spinner_chars = ['|', '/', '-', '\\'];
    let mut idx = 0;
    loop {
        eprint!(
            "\rUploading {} {}",
            spinner_chars[idx],
            progress.lock().await.bar()
        );
        idx = (idx + 1) % spinner_chars.len();
        io::stdout().flush().unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn run_upload_tasks<R>(
    promises: Vec<impl Future<Output = anyhow::Result<R>>>,
    progress: Arc<Mutex<ProgressState>>,
) -> anyhow::Result<Vec<R>> {
    let upload_feedback = tokio::task::spawn(display_upload_feedback(progress));

    defer! {
        upload_feedback.abort(); // Abort the spinner loop as download completes
        eprint!("\r\x1B[2K"); // clear the line
    }

    // Use try_join to run all promises in parallel and handle failures
    try_join_all(promises).await
}

pub async fn upload_file(
    r2: R2D2,
    file_path: String,
    bucket: Option<String>,
) -> anyhow::Result<()> {
    let path = Path::new(&file_path);
    let key = path
        .file_name()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default();
    let Ok(metadata) = tokio::fs::metadata(path).await else {
        bail!("`{file_path}` does not seem to exist.");
    };

    let file_size = metadata.len();

    let bucket = r2.bucket(&bucket)?;
    let client = r2.into_s3()?;

    let multipart_upload_res = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .with_context(|| {
            format!("Something went wrong trying to upload to {}. Are you sure you have the right credentials and bucket name?", &bucket)
        })?;

    let Some(upload_id) = multipart_upload_res.upload_id() else {
        bail!("No upload ID, can't continue");
    };

    let mut chunk_count = (file_size / CHUNK_SIZE) + 1;
    let mut size_of_last_chunk = file_size % CHUNK_SIZE;
    if size_of_last_chunk == 0 {
        size_of_last_chunk = CHUNK_SIZE;
        chunk_count -= 1;
    }

    if file_size == 0 {
        bail!("Bad file size (0).");
    }
    if chunk_count > MAX_CHUNKS {
        bail!("Too many chunks! Try increasing your chunk size.");
    }

    let to_upload = MultipartUpload::new(&client, path, upload_id, key, &bucket);

    let mut promises = vec![];
    let progress = Arc::new(Mutex::new(ProgressState::new(file_size)));

    for chunk_index in 0..chunk_count {
        // Chunk index needs to start at 0, but part numbers start at 1.
        let part_number = (chunk_index as i32) + 1;

        let this_chunk = if chunk_count - 1 == chunk_index {
            size_of_last_chunk
        } else {
            CHUNK_SIZE
        };

        let promise = upload_chunk(
            &to_upload,
            part_number,
            chunk_index,
            this_chunk, // -> chunk size
            Arc::clone(&progress),
        );

        promises.push(promise);
    }

    let upload_parts = run_upload_tasks(promises, Arc::clone(&progress)).await?;

    let completed_multipart_upload: CompletedMultipartUpload = CompletedMultipartUpload::builder()
        .set_parts(Some(upload_parts))
        .build();

    let _complete_multipart_upload_res = client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .multipart_upload(completed_multipart_upload)
        .upload_id(upload_id)
        .send()
        .await
        .with_context(|| "Something went wrong completing the upload.")?;

    eprintln!("ok?");

    Ok(())
}
