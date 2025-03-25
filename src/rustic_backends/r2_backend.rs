use bytes::Bytes;
use opendal::Operator;
use opendal::services::S3 as S3Builder;
use rustic_core::{
    ErrorKind, FileType, Id, ReadBackend, RepositoryBackends, RusticError, RusticResult,
    WriteBackend,
};
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::runtime::Runtime;
use tokio::task;
use typed_path::UnixPathBuf;

/// Uses opendal async instead of blocking
#[derive(Clone, Debug)]
pub struct R2Backend {
    account_id: String,
    key_id: String,
    secret: String,
    bucket: String,
    operator: Operator,
}

impl R2Backend {
    fn s3_builder(
        account_id: &str,
        key_id: &str,
        secret: &str,
        bucket: &str,
    ) -> S3Builder {
        S3Builder::default()
            // set the storage bucket for OpenDAL
            .root("/")
            .region("auto")
            .endpoint(&format!("https://{account_id}.r2.cloudflarestorage.com"))
            .access_key_id(key_id)
            .secret_access_key(secret)
            .bucket(bucket)
    }
    // fn as_s3_builder(&self) -> S3Builder {
    //     Self::s3_builder(&self.account_id, &self.key_id, &self.secret, &self.bucket)
    // }

    pub fn try_new(
        account_id: String,
        key_id: String,
        secret: String,
        bucket: String,
    ) -> anyhow::Result<Self> {
        let builder = Self::s3_builder(&account_id, &key_id, &secret, &bucket);

        let async_op: Operator = Operator::new(builder)?.finish();

        Ok(Self {
            account_id,
            key_id,
            secret,
            bucket,
            operator: async_op,
        })
    }

    pub fn to_backends(self) -> RepositoryBackends {
        RepositoryBackends::new(Arc::new(self), None)
    }

    // code from `https://github.com/rustic-rs/rustic_core/blob/13587a2d5fe3b708544b76c3a9539a6906356ecb/crates/backend/src/opendal.rs`
    // but using non-blocking operator (since we're already in tokio)

    async fn list_with_size_async(
        &self,
        tpe: FileType,
    ) -> RusticResult<Vec<(Id, u32)>> {
        if tpe == FileType::Config {
            return match self.operator.stat("config").await {
                Ok(entry) => Ok(vec![(
                    Id::default(),
                    entry.content_length().try_into().map_err(|err| {
                        RusticError::with_source(
                            ErrorKind::Internal,
                            "Parsing content length `{length}` failed",
                            err,
                        )
                            .attach_context("length", entry.content_length().to_string())
                    })?,
                )]),
                Err(err) if err.kind() == opendal::ErrorKind::NotFound => Ok(Vec::new()),
                Err(err) => Err(err).map_err(|err|
                    RusticError::with_source(
                        ErrorKind::Backend,
                        "Getting Metadata of type `{type}` failed in the backend. Please check if `{type}` exists.",
                        err,
                    )
                        .attach_context("type", tpe.to_string())
                ),
            };
        }

        let path = tpe.dirname().to_string() + "/";
        Ok(self
            .operator
            .list_with(&path)
            .recursive(true)
            .await
            .map_err(|err|
                RusticError::with_source(
                    ErrorKind::Backend,
                    "Listing all files of `{type}` in directory `{path}` and their sizes failed in the backend. Please check if the given path is correct.",
                    err,
                )
                    .attach_context("path", path)
                    .attach_context("type", tpe.to_string())
            )?
            .into_iter()
            .filter(|e| e.metadata().is_file())
            .map(|e| -> RusticResult<(Id, u32)> {
                Ok((
                    e.name().parse()?,
                    e.metadata()
                        .content_length()
                        .try_into()
                        .map_err(|err|
                            RusticError::with_source(
                                ErrorKind::Internal,
                                "Parsing content length `{length}` failed",
                                err,
                            )
                                .attach_context("length", e.metadata().content_length().to_string())
                        )?,
                ))
            })
            .inspect(|r| {
                if let Err(err) = r {
                    eprintln!("Error while listing files: {}", err.display_log());
                }
            })
            .filter_map(RusticResult::ok)
            .collect())
    }

    fn path(
        &self,
        tpe: FileType,
        id: &Id,
    ) -> String {
        let hex_id = id.to_hex();
        match tpe {
            FileType::Config => UnixPathBuf::from("config"),
            FileType::Pack => UnixPathBuf::from("data")
                .join(&hex_id[0..2])
                .join(&hex_id[..]),
            _ => UnixPathBuf::from(tpe.dirname()).join(&hex_id[..]),
        }
        .to_string()
    }

    async fn read_full_async(
        &self,
        tpe: FileType,
        id: &Id,
    ) -> RusticResult<Bytes> {
        let path = self.path(tpe, id);
        Ok(self
            .operator
            .read(&path)
            .await
            .map_err(|err|
                RusticError::with_source(
                    ErrorKind::Backend,
                    "Reading file `{path}` failed in the backend. Please check if the given path is correct.",
                    err,
                )
                    .attach_context("path", path)
                    .attach_context("type", tpe.to_string())
                    .attach_context("id", id.to_string())
            )?
            .to_bytes())
    }

    async fn read_partial_async(
        &self,
        tpe: FileType,
        id: &Id,
        _cacheable: bool,
        offset: u32,
        length: u32,
    ) -> RusticResult<Bytes> {
        let range = u64::from(offset)..u64::from(offset + length);
        let path = self.path(tpe, id);

        Ok(self
            .operator
            .read_with(&path)
            .range(range)
            .await
            .map_err(|err|
                RusticError::with_source(
                    ErrorKind::Backend,
                    "Partially reading file `{path}` failed in the backend. Please check if the given path is correct.",
                    err,
                )
                    .attach_context("path", path)
                    .attach_context("type", tpe.to_string())
                    .attach_context("id", id.to_string())
                    .attach_context("offset", offset.to_string())
                    .attach_context("length", length.to_string())
            )?
            .to_bytes())
    }

    async fn write_bytes_async(
        &self,
        tpe: FileType,
        id: &Id,
        _cacheable: bool,
        buf: Bytes,
    ) -> RusticResult<()> {
        let filename = self.path(tpe, id);
        self.operator.write(&filename, buf).await.map_err(|err| {
            RusticError::with_source(
                ErrorKind::Backend,
                "Writing file `{path}` failed in the backend. Please check if the given path is correct.",
                err,
            )
                .attach_context("path", filename)
                .attach_context("type", tpe.to_string())
                .attach_context("id", id.to_string())
        })?;

        Ok(())
    }

    async fn remove_async(
        &self,
        tpe: FileType,
        id: &Id,
        _cacheable: bool,
    ) -> RusticResult<()> {
        let filename = self.path(tpe, id);
        self.operator.delete(&filename).await.map_err(|err| {
            RusticError::with_source(
                ErrorKind::Backend,
                "Deleting file `{path}` failed in the backend. Please check if the given path is correct.",
                err,
            )
                .attach_context("path", filename)
                .attach_context("type", tpe.to_string())
                .attach_context("id", id.to_string())
        })?;
        Ok(())
    }
}

/// Use tokio to run async code, either in an existing context or a new runtime
macro_rules! block_on_in_place {
    ($expr:expr) => {{
        match Handle::try_current() {
            Ok(handle) => task::block_in_place(|| handle.block_on(async { $expr })),
            Err(_) => {
                let runtime = Runtime::new().map_err(|err| {
                    RusticError::with_source(
                        ErrorKind::Internal,
                        "Failed to create Tokio runtime",
                        err,
                    )
                })?;
                runtime.block_on(async { $expr })
            },
        }
    }};
}

impl ReadBackend for R2Backend {
    fn location(&self) -> String {
        format!("https://{}.r2.cloudflarestorage.com", self.account_id)
    }

    // Forward to async functions using existing runtime

    fn list_with_size(
        &self,
        tpe: FileType,
    ) -> RusticResult<Vec<(Id, u32)>> {
        block_on_in_place!(self.list_with_size_async(tpe).await)
    }

    fn read_full(
        &self,
        tpe: FileType,
        id: &Id,
    ) -> RusticResult<Bytes> {
        block_on_in_place!(self.read_full_async(tpe, id).await)
    }

    fn read_partial(
        &self,
        tpe: FileType,
        id: &Id,
        cacheable: bool,
        offset: u32,
        length: u32,
    ) -> RusticResult<Bytes> {
        block_on_in_place!(
            self.read_partial_async(tpe, id, cacheable, offset, length)
                .await
        )
    }
}

impl WriteBackend for R2Backend {
    fn write_bytes(
        &self,
        tpe: FileType,
        id: &Id,
        cacheable: bool,
        buf: Bytes,
    ) -> RusticResult<()> {
        // eprintln!("uploading {id}");
        block_on_in_place!(self.write_bytes_async(tpe, id, cacheable, buf).await)

        // match Handle::try_current() {
        //     Ok(handle) => tokio::task::block_in_place(|| {
        //         handle.block_on(async { self.write_bytes_async(tpe, id, cacheable, buf).await })
        //     }),
        //     Err(_) => {
        //         let rt = tokio::runtime::Runtime::new().expect("no tokio :("); // Create a Tokio runtime if needed
        //         rt.block_on(async { self.write_bytes_async(tpe, id, cacheable, buf).await })
        //     },
        // }
    }

    fn remove(
        &self,
        tpe: FileType,
        id: &Id,
        cacheable: bool,
    ) -> RusticResult<()> {
        block_on_in_place!(self.remove_async(tpe, id, cacheable).await)
    }
}
