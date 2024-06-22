use crate::helpers::{IntoPythonError, ResultToString};
use dotenvy::from_path_iter;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::PyResult;
use reqwest::header::HeaderMap;
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fmt::Display;
use std::io::BufReader;
use std::path::Path;
use url::Url;

const CLOUDFLARE_API: &str = "https://api.cloudflare.com/client/v4";

fn get_from_config(
    config: &BTreeMap<String, String>,
    key: &str,
) -> Result<String, String> {
    config.get(key).map_or_else(
        || Err(format!("Key {key} could not be found in the config.")),
        |value| Ok(value.clone()),
    )
}

pub fn get_from_env(key: &str) -> Result<String, String> {
    env::var(key).map_err(|_| format!("Key {key} could not be found in your environment."))
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
    buckets: Vec<BucketListData>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BucketListData {
    name: String,
    creation_date: String,
    location: Option<String>,
    storage_class: Option<String>,
}

trait SendAndHandle {
    async fn send_and_handle(self) -> Result<String, String>;
    async fn send_and_parse<T: serde::de::DeserializeOwned>(self)
        -> Result<ApiResponse<T>, String>;
}

impl SendAndHandle for RequestBuilder {
    async fn send_and_handle(self) -> Result<String, String> {
        let resp = self.send().await;

        match resp {
            Ok(resp) => resp
                .text()
                .await
                .map_err(|e| format!("Error reading response text: {e}")),
            Err(e) => Err(format!("Request error: {e}")),
        }
    }

    async fn send_and_parse<T: serde::de::DeserializeOwned>(
        self
    ) -> Result<ApiResponse<T>, String> {
        let text = self.send_and_handle().await?;

        dbg!(&text);

        let buffer = BufReader::new(text.as_bytes());
        // normally, `serde_json::from_string` expects &str but that requires a lifetime
        // and `text` doesn't live that long.
        // So using `serde owned` with a buffer works better in this scenario.
        let result: ApiResponse<T> = serde_json::de::from_reader(buffer).map_err_to_string()?;

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

#[derive(Debug, Default)]
pub struct ListOptions {
    cursor: Option<String>,
    direction: Option<Direction>,
    order: Option<String>,
    per_page: Option<u32>,
    start_after: Option<String>,
}

impl QueryString for ListOptions {
    fn to_query(&self) -> String {
        let mut parts = vec![];

        if let Some(cursor) = &self.cursor {
            parts.push(to_query_part("cursor", cursor));
        }

        if let Some(direction) = &self.direction {
            parts.push(to_query_part("direction", direction.to_string()));
        }

        if let Some(order) = &self.order {
            parts.push(to_query_part("order", order));
        }

        if let Some(per_page) = &self.per_page {
            parts.push(to_query_part("per_page", per_page.to_string()));
        }

        if let Some(start_after) = &self.start_after {
            parts.push(to_query_part("start_after", start_after));
        }

        parts.join("&")
    }
}

pub struct R2D2 {
    account_id: String,
    apikey: String,
    pub bucket: Option<String>,
    // key id and secret?
}

impl R2D2 {
    // low-level: config, setup stuff:

    pub fn from_path(path: &Path) -> Result<Self, String> {
        if let Some(config) = read_configfile(path) {
            Ok(Self {
                account_id: get_from_config(&config, "R2_ACCOUNT_ID")?,
                apikey: get_from_config(&config, "R2_API_KEY")?,
                bucket: get_from_config(&config, "R2_BUCKET").ok(),
            })
        } else {
            Err(format!("Invalid config file {}", ".r2"))
        }
    }

    pub fn from_filename(filename: &str) -> Result<Self, String> {
        let path = Path::new(filename);
        Self::from_path(path)
    }

    /// Read .r2 config file
    pub fn from_dot_r2() -> Result<Self, String> {
        Self::from_filename(".r2")
    }

    pub fn from_dotenv() -> Result<Self, String> {
        Self::from_filename(".env")
    }

    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            account_id: get_from_env("R2_ACCOUNT_ID")?,
            apikey: get_from_env("R2_API_KEY")?,
            bucket: get_from_env("R2_BUCKET").ok(),
        })
    }

    pub fn guess() -> Result<Self, String> {
        // .r2, then .env, then environment variables
        Self::from_dot_r2()
            .or_else(|_| Self::from_dotenv())
            .or_else(|_| Self::from_env())
            .map_err(|e| {
                format!("No config could be found (tried .r2, .env, environment variables) - {e}")
            })
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

    pub fn set_bucket(
        &mut self,
        bucket: Option<String>,
    ) {
        self.bucket = bucket;
    }

    // medium level (api endpoints):

    pub async fn _usage(&self) -> Result<ApiResponse<UsageResultData>, String> {
        let Some(bucket) = &self.bucket else {
            return Err("Can't use `usage()` without a bucket!".to_string());
        };

        let Some(request) = self.request(&format!("buckets/{bucket}/usage?=null")) else {
            return Err(format!("Request for '{}' could not be set up.", "usage"));
        };

        request.send_and_parse().await
    }

    /// Show usage for current bucket
    pub async fn usage(&self) -> PyResult<UsageResultData> {
        self._usage()
            .await
            .to_python_error("usage")?
            .to_python_error("usage")
    }

    pub async fn _list(
        &self,
        options: Option<ListOptions>,
    ) -> Result<ApiResponse<BucketResultData>, String> {
        // todo: cursor (for pagination), direction, name_contains, order, per_page, start_after
        let Some(request) = self.request("buckets") else {
            return Err(format!("Request for '{}' could not be set up.", "usage"));
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
