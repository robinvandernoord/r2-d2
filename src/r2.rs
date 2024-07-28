use crate::commands::list::ListOptions;
use crate::helpers::IntoPythonError;
use anyhow::{anyhow, bail, Context};
use aws_config::{Region, SdkConfig};
use aws_credential_types::provider::error::CredentialsError;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_credential_types::{provider, Credentials};
use aws_sdk_s3::config::ProvideCredentials;
use aws_sdk_s3::Client as S3Client;
use aws_smithy_runtime_api::client::stalled_stream_protection::StalledStreamProtectionConfig;
use dotenvy::from_path_iter;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::PyResult;
use reqwest::header::HeaderMap;
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fmt::{Debug, Display};
use std::io::BufReader;
use std::path::Path;
use url::Url;

const CLOUDFLARE_API: &str = "https://api.cloudflare.com/client/v4";

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

fn read_configfile(path: &Path) -> Option<BTreeMap<String, String>> {
    let iter = from_path_iter(path).ok()?;

    let mut config: BTreeMap<String, String> = BTreeMap::new();

    for item in iter {
        let (key, value) = item.ok()?;
        config.insert(key, value);
    }

    Some(config)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub errors: Option<Vec<ApiError>>,
    pub messages: Option<Vec<String>>,
    pub result: Option<T>,
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

        Err(
            self.errors.as_ref().map_or_else(|| PyValueError::new_err(format!(
                "Something went wrong for {hint}, but no specific information was provided."
            )), |errors| {
                if errors.is_empty() {
                    PyValueError::new_err(format!("Something went wrong for {hint}, but no specific information was provided."))
                } else {
                    let mut msg = format!("[{hint}] ");

                    for error in errors {
                        msg.push_str(&error.message);
                        msg.push('\n');
                    }

                    PyValueError::new_err(msg)
                }
            })
        )
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiError {
    pub code: i32,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
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

#[derive(Serialize, Deserialize, Debug)]
pub struct BucketResultData {
    pub buckets: Vec<BucketListData>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BucketListData {
    pub name: String,
    pub creation_date: String,
    pub location: Option<String>,
    pub storage_class: Option<String>,
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

#[derive(Debug)]
pub struct R2D2 {
    account_id: String,
    apikey: String,
    aws_access_key_id: Option<String>,
    aws_secret_access_key: Option<String>,
    pub bucket: Option<String>,
    // key id and secret?
}

impl R2D2 {
    // low-level: config, setup stuff:

    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        if let Some(config) = read_configfile(path) {
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
            .with_context(|| "No config could be found (tried .r2, .env, environment variables)")
    }

    pub fn bucket(
        &self,
        bucket: &Option<String>,
    ) -> anyhow::Result<String> {
        let Some(bucket) = bucket.as_ref().or(self.bucket.as_ref()) else {
            bail!("Bucket (`R2_BUCKET`) required for this operation.");
        };

        Ok(bucket.to_string())
    }

    pub fn into_s3(self) -> anyhow::Result<S3Client> {
        let url = self.endpoint_url();
        let region = Region::from_static("auto");
        let provider = SharedCredentialsProvider::new(self);

        let shared_config = SdkConfig::builder()
            .region(Some(region))
            .endpoint_url(url)
            .credentials_provider(provider)
            .stalled_stream_protection(
                // allow low throughput (otherwise you get DispatchFailure/ThroughputBelowMinimum)
                StalledStreamProtectionConfig::enabled()
                    .upload_enabled(false)
                    .build(),
            )
            .build();

        Ok(S3Client::new(&shared_config))
    }

    pub fn endpoint_url(&self) -> String {
        format!("https://{}.r2.cloudflarestorage.com", &self.account_id,)
    }

    fn _base_url(&self) -> String {
        format!("{}/accounts/{}/r2/", CLOUDFLARE_API, self.account_id,)
    }

    pub fn build_url(
        &self,
        endpoint: &str,
    ) -> Option<Url> {
        let base = Url::parse(&self._base_url()).ok()?;
        base.join(endpoint).ok()
    }

    pub fn request(
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

    #[allow(clippy::unused_async)]
    pub async fn aws_credentials(&self) -> Result<Credentials, CredentialsError> {
        let (Some(key_id), Some(secret)) = (&self.aws_access_key_id, &self.aws_secret_access_key)
        else {
            return Err(CredentialsError::invalid_configuration(
                "`R2_ACCESS_KEY_ID` and `R2_SECRET_ACCESS_KEY` are required for `upload`.",
            ));
        };

        Ok(Credentials::new(key_id, secret, None, None, "r2-d2"))
    }

    pub fn set_bucket(
        &mut self,
        bucket: Option<String>,
    ) {
        self.bucket = bucket;
    }

    // medium level (api endpoints):

    pub async fn _usage(
        &self,
        bucket: Option<String>,
    ) -> anyhow::Result<ApiResponse<UsageResultData>> {
        let bucket = self.bucket(&bucket)?;

        let Some(request) = self.request(&format!("buckets/{bucket}/usage?=null")) else {
            bail!("Request for '{}' could not be set up.", "usage");
        };

        request.send_and_parse().await
    }

    /// Show usage for current bucket
    pub async fn usage(
        &self,
        bucket: Option<String>,
    ) -> PyResult<UsageResultData> {
        self._usage(bucket)
            .await
            .to_python_error("usage")?
            .to_python_error("usage")
    }

    pub async fn _list(
        &self,
        options: Option<ListOptions>,
    ) -> anyhow::Result<ApiResponse<BucketResultData>> {
        // todo: cursor (for pagination), direction, name_contains, order, per_page, start_after
        let Some(request) = self.request("buckets") else {
            bail!("Request for '{}' could not be set up.", "usage");
        };

        let options = options.unwrap_or_default();

        dbg!(options);

        request.send_and_parse().await
    }

    /// List buckets
    pub async fn list(
        &self,
        options: Option<ListOptions>,
    ) -> PyResult<Vec<BucketListData>> {
        // todo: cursor (for pagination), direction, name_contains, order, per_page, start_after
        let data = self
            ._list(options)
            .await
            .to_python_error("list")?
            .to_python_error("list")?;

        Ok(data.buckets)
    }
}

impl ProvideCredentials for R2D2 {
    fn provide_credentials<'a>(&'a self) -> provider::future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        provider::future::ProvideCredentials::new(self.aws_credentials())
    }
}
