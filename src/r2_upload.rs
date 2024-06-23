use std::path::Path;

use aws_config::Region;
use aws_config::SdkConfig;
use aws_sdk_s3::config::SharedCredentialsProvider;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_sdk_s3::Client as S3Client;
use aws_smithy_types::byte_stream::{ByteStream, Length};

use crate::r2::R2D2;

// In bytes, minimum chunk size of 50 MB. Increase CHUNK_SIZE to send larger chunks.
const CHUNK_SIZE: u64 = 1024 * 1024 * 10;
const MAX_CHUNKS: u64 = 10000;

pub async fn upload_example(
    r2: R2D2,
    file_path: String,
    bucket: Option<String>,
) -> Result<(), String> {
    let path = Path::new(&file_path);
    let key = path
        .file_name()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default();
    let Ok(metadata) = tokio::fs::metadata(path).await else {
        return Err(format!("`{}` does not seem to exist.", file_path));
    };

    let file_size = metadata.len();

    let bucket = r2.bucket(bucket)?;

    // let url = r2.endpoint_url(Some(bucket.clone()))?;
    let url = r2.endpoint_url()?;
    let region = Region::from_static("auto");
    let provider = SharedCredentialsProvider::new(r2);

    let shared_config = SdkConfig::builder()
        .region(Some(region))
        .endpoint_url(url)
        .credentials_provider(provider)
        .build();

    let client = S3Client::new(&shared_config);

    let multipart_upload_res = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .map_err(|err| {
            // dbg!(err.raw_response().unwrap().body());

            format!("Something went wrong trying to upload to {}. Are you sure you have the right credentials and bucket name?", &bucket)
        })?;

    let Some(upload_id) = multipart_upload_res.upload_id() else {
        return Err("No upload ID, can't continue".to_string());
    };

    let mut chunk_count = (file_size / CHUNK_SIZE) + 1;
    let mut size_of_last_chunk = file_size % CHUNK_SIZE;
    if size_of_last_chunk == 0 {
        size_of_last_chunk = CHUNK_SIZE;
        chunk_count -= 1;
    }

    if file_size == 0 {
        return Err("Bad file size (0).".to_string());
    }
    if chunk_count > MAX_CHUNKS {
        return Err("Too many chunks! Try increasing your chunk size.".to_string());
    }

    let mut upload_parts: Vec<CompletedPart> = Vec::new();

    // for chunk_index in 0..chunk_count {
    //     let part_number = (chunk_index as i32) + 1;
    //     eprintln!("{}/{}", part_number, chunk_count);
    //
    //     let this_chunk = if chunk_count - 1 == chunk_index {
    //         size_of_last_chunk
    //     } else {
    //         CHUNK_SIZE
    //     };
    //     let stream = ByteStream::read_from()
    //         .path(path)
    //         .offset(chunk_index * CHUNK_SIZE)
    //         .length(Length::Exact(this_chunk))
    //         .build()
    //         .await
    //         .map_err_to_string()?;
    //
    //     // Chunk index needs to start at 0, but part numbers start at 1.
    //     let upload_part_res = client
    //         .upload_part()
    //         .key(key)
    //         .bucket(&bucket)
    //         .upload_id(upload_id)
    //         .body(stream)
    //         .part_number(part_number)
    //         .send()
    //         .await
    //         .map_err(|err| {
    //             dbg!(&err);
    //             format!("Something went wrong uploading part {}/{}.", part_number, chunk_count)
    //         })?;
    //
    //     upload_parts.push(
    //         CompletedPart::builder()
    //             .e_tag(upload_part_res.e_tag.unwrap_or_default())
    //             .part_number(part_number)
    //             .build(),
    //     );
    // }

    for chunk_index in 0..chunk_count {
        // todo: in parallel?
        let part_number = (chunk_index as i32) + 1;
        eprintln!("{}/{}", part_number, chunk_count);

        let this_chunk = if chunk_count - 1 == chunk_index {
            size_of_last_chunk
        } else {
            CHUNK_SIZE
        };

        let stream = ByteStream::read_from()
            .path(path)
            .offset(chunk_index * CHUNK_SIZE)
            // .length(Length::UpTo(CHUNK_SIZE))
            .length(Length::Exact(this_chunk))
            .build()
            .await
            .unwrap();

        let upload_part_res = client
            .upload_part()
            .key(key)
            .bucket(&bucket)
            .upload_id(upload_id)
            .body(stream)
            .part_number(part_number)
            .send()
            .await
            .unwrap();

        let e_tag = upload_part_res.e_tag.unwrap_or_default();

        upload_parts.push(
            CompletedPart::builder()
                .e_tag(e_tag)
                .part_number(part_number)
                .build(),
        );
    }

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
        .map_err(|err| {
            dbg!(&err);
            dbg!(err.raw_response());
            "Something went wrong completing the upload.".to_string()
        })?;

    dbg!(_complete_multipart_upload_res);

    Ok(())
}
