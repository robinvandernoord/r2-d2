use crate::commands::list::ListOptions;
use crate::helpers::IntoPythonError;
use crate::rustic_backends::r2_backend::R2Backend;
use crate::rustic_progress::ProgressBar;
use anyhow::{Context, anyhow, bail};
use dotenvy::from_path_iter;
use pyo3::PyResult;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use reqwest::header::HeaderMap;
use reqwest::{Client, RequestBuilder};
use resolve_path::PathResolveExt;
use rustic_core::{Repository, RepositoryOptions};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fmt::{Debug, Display};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use url::Url;

const CLOUDFLARE_API: &str = "https://api.cloudflare.com/client/v4/";

fn get_from_config(
    config: &BTreeMap<String, String>,
    key: &str,
) -> anyhow::Result<String> {
    config.get(key).map_or_else(
        || Err(anyhow!("Key {key} could not be found in the config.")),
        |value| Ok(value.clone()),
    )
}

pub fn get_from_env(key: &str) -> anyhow::Result<String> {
    env::var(key).map_err(|_| anyhow!("Key {key} could not be found in your environment."))
}

fn read_configfile(path: &PathBuf) -> Option<BTreeMap<String, String>> {
    let iter = from_path_iter(path).ok()?;

    let mut config: BTreeMap<String, String> = BTreeMap::new();

    for item in iter {
        let (key, value) = item.ok()?;
        config.insert(key, value);
    }

    Some(config)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub errors: Option<Vec<ApiError>>,
    // pub messages: Option<Vec<String>>,
    pub messages: Option<Vec<Message>>,
    pub result: Option<T>,
}

impl<T> ApiResponse<T> {
    fn into_error(self) -> anyhow::Result<T> {
        if self.success {
            return self.result.map_or_else(
                || Err(anyhow!("Expected result data but got None!",)),
                |data| Ok(data),
            );
        }

        Err(self.errors.as_ref().map_or_else(
            || anyhow!("Something went wrong, but no specific information was provided."),
            |errors| {
                if errors.is_empty() {
                    anyhow!("Something went wrong, but no specific information was provided.")
                } else {
                    let mut msg = String::new();

                    for error in errors {
                        msg.push_str(&error.message);
                        msg.push('\n');
                    }

                    anyhow!("{msg}")
                }
            },
        ))
    }
}

impl<T> IntoPythonError<T> for ApiResponse<T> {
    fn to_python_error(
        self,
        hint: &str,
    ) -> PyResult<T> {
        if self.success {
            return self.result.map_or_else(
                || {
                    Err(PyRuntimeError::new_err(
                        "Expected result data but got None!",
                    ))
                },
                |data| Ok(data),
            );
        }

        Err(self.errors.as_ref().map_or_else(
            || {
                PyValueError::new_err(format!(
                    "Something went wrong for {hint}, but no specific information was provided."
                ))
            },
            |errors| {
                if errors.is_empty() {
                    PyValueError::new_err(format!(
                        "Something went wrong for {hint}, but no specific information was provided."
                    ))
                } else {
                    let mut msg = format!("[{hint}] ");

                    for error in errors {
                        msg.push_str(&error.message);
                        msg.push('\n');
                    }

                    PyValueError::new_err(msg)
                }
            },
        ))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApiError {
    pub code: i32,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UsageResultData {
    pub end: Option<String>,

    #[serde(rename = "payloadSize")]
    pub payload_size: Option<String>,

    #[serde(rename = "metadataSize")]
    pub metadata_size: Option<String>,

    #[serde(rename = "objectCount")]
    pub object_count: Option<String>,

    #[serde(rename = "uploadCount")]
    pub upload_count: Option<String>,

    #[serde(rename = "infrequentAccessPayloadSize")]
    pub infrequent_access_payload_size: Option<String>,

    #[serde(rename = "infrequentAccessMetadataSize")]
    pub infrequent_access_metadata_size: Option<String>,

    #[serde(rename = "infrequentAccessObjectCount")]
    pub infrequent_access_object_count: Option<String>,

    #[serde(rename = "infrequentAccessUploadCount")]
    pub infrequent_access_upload_count: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BucketResultData {
    pub buckets: Vec<BucketData>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BucketData {
    pub name: String,
    #[serde(rename = "creationDate")]
    pub creation_date: String,
    pub location: Option<String>,
    #[serde(rename = "storageClass")]
    pub storage_class: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ManagedBucketDomainData {
    #[serde(rename = "bucketId")]
    pub bucket_id: String,
    pub domain: String,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CustomBucketDomainList {
    pub domains: Vec<CustomBucketDomainData>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CustomBucketDomainData {
    pub domain: String,
    pub enabled: bool,
    pub status: Option<CustomBucketDomainStatus>,
    // minDNS, zoneId, zoneName
}
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CustomBucketDomainStatus {
    ownership: String,
    ssl: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TokenVerifyData {
    pub id: String,
    pub status: String,
}

impl TokenVerifyData {
    pub fn ok(&self) -> bool {
        self.status == "active"
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Message {
    #[serde(rename = "code")]
    pub code: u32,

    #[serde(rename = "message")]
    pub message: String,

    #[serde(rename = "type")]
    pub message_type: Option<String>,
}

trait SendAndHandle {
    async fn send_and_handle(self) -> anyhow::Result<String>;
    async fn send_and_parse<T: serde::de::DeserializeOwned>(self)
    -> anyhow::Result<ApiResponse<T>>;
}

impl SendAndHandle for RequestBuilder {
    async fn send_and_handle(self) -> anyhow::Result<String> {
        let resp = self.send().await;

        match resp {
            Ok(resp) => resp
                .text()
                .await
                .with_context(|| "Error reading response text"),
            Err(e) => Err(e).with_context(|| "Request error"),
        }
    }

    async fn send_and_parse<T: serde::de::DeserializeOwned>(
        self
    ) -> anyhow::Result<ApiResponse<T>> {
        let text = self.send_and_handle().await?;
        let buffer = BufReader::new(text.as_bytes());
        // normally, `serde_json::from_string` expects &str but that requires a lifetime
        // and `text` doesn't live that long.
        // So using `serde owned` with a buffer works better in this scenario.
        let result: ApiResponse<T> = serde_json::de::from_reader(buffer)?;

        Ok(result)
    }
}

#[derive(Debug)]
pub enum Direction {
    ASC,
    DESC,
}

impl Direction {
    pub const fn default() -> Self {
        Self::ASC
    }
}

impl Display for Direction {
    // instead of ToString
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        let str = match self {
            Self::ASC => String::from("asc"),
            Self::DESC => String::from("desc"),
        };
        write!(f, "{str}")
    }
}

impl From<Direction> for String {
    fn from(val: Direction) -> Self {
        val.to_string()
    }
}

pub trait QueryString {
    fn to_query(&self) -> String;
}

pub fn to_query_part<T: Into<String>>(
    key: &str,
    value: T,
) -> String {
    format!("{}={}", key, value.into())
}

pub type ResticRepository = Repository<ProgressBar, ()>;
// pub type ResticRepository = Repository<ProgressOptions, ()>;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct R2D2 {
    account_id: String,
    apikey: String,
    aws_access_key_id: Option<String>,
    aws_secret_access_key: Option<String>,
    pub bucket: Option<String>,
    // todo: repo password
}

macro_rules! bucket_request {
    ($self:expr, $bucket:expr) => {{
        let bucket = $self.bucket_or(&$bucket)?;
        let path = format!("buckets/{}", bucket);

        let Some(request) = $self.request_get(&path) else {
            bail!("Request for '{}' could not be set up.", path);
        };

        request.send_and_parse().await
    }};
    ($self:expr, $bucket:expr, $($path:expr),+) => {{
        let bucket = $self.bucket_or(&$bucket)?;
        let path = format!("buckets/{}{}", bucket, concat!("/", $("/", $path),*));

        let Some(request) = $self.request_get(&path) else {
            bail!("Request for '{}' could not be set up.", path);
        };

        request.send_and_parse().await
    }};
}

macro_rules! api_to_python {
    ($self:expr, $endpoint:ident $(, $args:expr)*) => {
        $self
            .$endpoint($($args),*)
            .await
            .to_python_error(stringify!($endpoint))?
            .to_python_error(stringify!($endpoint))
    };
}

impl R2D2 {
    // low-level: config, setup stuff:

    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        let abs_path = path
            .try_resolve()
            .map_or_else(|_| path.to_path_buf(), std::borrow::Cow::into_owned);

        if let Some(config) = read_configfile(&abs_path) {
            Ok(Self {
                account_id: get_from_config(&config, "R2_ACCOUNT_ID")?,
                apikey: get_from_config(&config, "R2_API_KEY")?,
                bucket: get_from_config(&config, "R2_BUCKET").ok(),
                aws_access_key_id: get_from_config(&config, "R2_ACCESS_KEY_ID").ok(),
                aws_secret_access_key: get_from_config(&config, "R2_SECRET_ACCESS_KEY").ok(),
            })
        } else {
            bail!("Invalid config file {}", ".r2")
        }
    }

    pub fn from_filename(filename: &str) -> anyhow::Result<Self> {
        let path = Path::new(filename);
        Self::from_path(path)
    }

    /// Read .r2 config file
    pub fn from_dot_r2() -> anyhow::Result<Self> {
        Self::from_filename(".r2")
    }

    pub fn from_global_dot_r2() -> anyhow::Result<Self> {
        Self::from_filename("~/.r2")
    }

    pub fn from_dotenv() -> anyhow::Result<Self> {
        Self::from_filename(".env")
    }

    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            account_id: get_from_env("R2_ACCOUNT_ID")?,
            apikey: get_from_env("R2_API_KEY")?,
            bucket: get_from_env("R2_BUCKET").ok(),
            aws_access_key_id: get_from_env("R2_ACCESS_KEY_ID").ok(),
            aws_secret_access_key: get_from_env("R2_SECRET_ACCESS_KEY").ok(),
        })
    }

    pub fn guess() -> anyhow::Result<Self> {
        // .r2, then .env, then environment variables
        Self::from_dot_r2()
            .or_else(|_| Self::from_dotenv())
            .or_else(|_| Self::from_env())
            .or_else(|_| Self::from_global_dot_r2())
            .with_context(|| "No config could be found (tried .r2, .env, environment variables)")
    }

    pub fn bucket_or(
        &self,
        bucket: &Option<String>,
    ) -> anyhow::Result<String> {
        let Some(bucket) = bucket.as_ref().or(self.bucket.as_ref()) else {
            bail!("Bucket (`R2_BUCKET`) required for this operation.");
        };

        Ok(bucket.to_string())
    }

    // /// `SharedCredentialsProvider` eats self so it needs to be owned.
    // pub fn into_s3(self) -> anyhow::Result<S3Client> {
    //     let url = self.endpoint_url();
    //     let region = Region::from_static("auto");
    //     let provider = SharedCredentialsProvider::new(self);
    //
    //     let shared_config = SdkConfig::builder()
    //         .region(Some(region))
    //         .endpoint_url(url)
    //         .credentials_provider(provider)
    //         .stalled_stream_protection(
    //             // allow low throughput (otherwise you get DispatchFailure/ThroughputBelowMinimum)
    //             StalledStreamProtectionConfig::enabled()
    //                 .upload_enabled(false)
    //                 .build(),
    //         )
    //         .build();
    //
    //     Ok(S3Client::new(&shared_config))
    // }

    pub fn into_opendal_backend(self) -> anyhow::Result<R2Backend> {
        R2Backend::try_new(
            self.account_id,
            self.aws_access_key_id.unwrap_or_default(),
            self.aws_secret_access_key.unwrap_or_default(),
            self.bucket.unwrap_or_default(),
        )
    }

    pub fn into_rustic(self) -> anyhow::Result<ResticRepository> {
        let repo_opts = RepositoryOptions::default().password("test");

        let backend = self.into_opendal_backend()?;
        let backends = backend.into_backends();

        let progress_bar = ProgressBar::default();
        let repo = Repository::new_with_progress(&repo_opts, &backends, progress_bar)?;

        Ok(repo)
    }

    pub fn endpoint_url(&self) -> String {
        format!("https://{}.r2.cloudflarestorage.com", &self.account_id,)
    }

    pub fn build_url(
        &self,
        endpoint: &str,
    ) -> Option<Url> {
        let mut endpoint = endpoint; // clone
        let base = if endpoint.starts_with('/') {
            endpoint = endpoint.strip_prefix("/").unwrap_or(endpoint);
            Url::parse(CLOUDFLARE_API).ok()?
        } else {
            let base_url = format!("{}/accounts/{}/r2/", CLOUDFLARE_API, self.account_id);
            Url::parse(&base_url).ok()?
        };

        base.join(endpoint).ok()
    }

    /// See: [https://developers.cloudflare.com/api/resources/r2/](https://developers.cloudflare.com/api/resources/r2/)
    pub fn request_get(
        &self,
        endpoint: &str,
    ) -> Option<RequestBuilder> {
        let client = Client::new();
        let url = self.build_url(endpoint)?.to_string();
        let request = client.get(url);

        self.headers().map(|headers| request.headers(headers))
    }

    pub fn headers(&self) -> Option<HeaderMap> {
        let mut headers = HeaderMap::new();
        let bearer = format!("Bearer {}", self.apikey);
        headers.insert("Authorization", bearer.parse().ok()?);

        Some(headers)
    }

    // /// this function is async becuase `provider::future::ProvideCredentials` expects that
    // #[allow(clippy::unused_async)]
    // pub async fn aws_credentials(&self) -> Result<Credentials, CredentialsError> {
    //     let (Some(key_id), Some(secret)) = (&self.aws_access_key_id, &self.aws_secret_access_key)
    //     else {
    //         return Err(CredentialsError::invalid_configuration(
    //             "`R2_ACCESS_KEY_ID` and `R2_SECRET_ACCESS_KEY` are required for `upload`.",
    //         ));
    //     };
    //
    //     Ok(Credentials::new(key_id, secret, None, None, "r2-d2"))
    // }

    pub fn set_bucket(
        &mut self,
        bucket: Option<String>,
    ) {
        self.bucket = bucket;
    }

    // medium level (api endpoints):

    pub async fn verify(&self) -> anyhow::Result<ApiResponse<TokenVerifyData>> {
        // start with /user so it doesn't prepend /accounts/<id>/r2:
        let Some(request) = self.request_get("/user/tokens/verify") else {
            bail!("Request for '{}' could not be set up.", "verify");
        };

        request.send_and_parse().await
    }
    pub async fn verify_py(&self) -> PyResult<TokenVerifyData> {
        api_to_python!(self, verify)
    }

    pub async fn bucket(
        &self,
        bucket: Option<String>,
    ) -> anyhow::Result<ApiResponse<BucketData>> {
        bucket_request!(self, bucket)
    }

    /// Show info for current bucket
    pub async fn bucket_py(
        &self,
        bucket: Option<String>,
    ) -> PyResult<BucketData> {
        api_to_python!(self, bucket, bucket)
    }

    pub async fn bucket_domains(
        &self,
        bucket: Option<String>,
    ) -> anyhow::Result<Vec<String>> {
        let mut domains = self
            .bucket_custom_domains(bucket.clone())
            .await
            .unwrap_or_default();

        if let Ok(managed) = self.bucket_managed_domain(bucket).await {
            domains.push(managed);
        }

        Ok(domains)
    }

    pub async fn bucket_domain(
        &self,
        bucket: Option<String>,
    ) -> Option<Url> {
        let Ok(mut domains) = self.bucket_domains(bucket).await else {
            return None;
        };

        let domain = domains.pop()?;
        let domain_with_schema = format!("https://{domain}");

        Url::parse(&domain_with_schema).ok()
    }

    pub async fn bucket_custom_domains(
        &self,
        bucket: Option<String>,
    ) -> anyhow::Result<Vec<String>> {
        let resp: CustomBucketDomainList =
            bucket_request!(self, bucket, "domains/custom")?.into_error()?;

        Ok(resp
            .domains
            .into_iter()
            .filter_map(|item| item.enabled.then_some(item.domain))
            .collect())
    }

    pub async fn bucket_managed_domain(
        &self,
        bucket: Option<String>,
    ) -> anyhow::Result<String> {
        let resp: ManagedBucketDomainData =
            bucket_request!(self, bucket, "domains/managed")?.into_error()?;

        if !resp.enabled {
            bail!("Managed domain disabled!")
        }

        Ok(resp.domain)
    }

    pub async fn usage(
        &self,
        bucket: Option<String>,
    ) -> anyhow::Result<ApiResponse<UsageResultData>> {
        bucket_request!(self, bucket, "/usage?=null")
    }

    /// Show usage for current bucket
    pub async fn usage_py(
        &self,
        bucket: Option<String>,
    ) -> PyResult<UsageResultData> {
        api_to_python!(self, usage, bucket)
    }

    pub async fn list(
        &self,
        options: Option<ListOptions>,
    ) -> anyhow::Result<ApiResponse<BucketResultData>> {
        // todo: cursor (for pagination), direction, name_contains, order, per_page, start_after
        let Some(request) = self.request_get("buckets") else {
            bail!("Request for '{}' could not be set up.", "usage");
        };

        let options = options.unwrap_or_default();

        dbg!(options);

        request.send_and_parse().await
    }

    /// List buckets
    pub async fn list_py(
        &self,
        options: Option<ListOptions>,
    ) -> PyResult<Vec<BucketData>> {
        // todo: cursor (for pagination), direction, name_contains, order, per_page, start_after
        let data = api_to_python!(self, list, options)?;

        Ok(data.buckets)
    }
}

// impl ProvideCredentials for R2D2 {
//     fn provide_credentials<'a>(&'a self) -> provider::future::ProvideCredentials<'a>
//     where
//         Self: 'a,
//     {
//         provider::future::ProvideCredentials::new(self.aws_credentials())
//     }
// }
