use std::collections::BTreeMap;
use std::path::Path;
use dotenvy::from_path_iter;
use reqwest::{Client, RequestBuilder};
use reqwest::header::HeaderMap;
use url::Url;
use serde::{Serialize, Deserialize};
use crate::helpers::ResultToString;

fn get_from_config(
    config: &BTreeMap<String, String>,
    key: &str,
) -> Result<String, String> {
    config.get(key).map_or_else(|| Err(format!("Key {key} could not be found in the config.")), |value| Ok(value.clone()))
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
    success: bool,
    errors: Option<Vec<ApiError>>,
    messages: Option<Vec<String>>,
    result: Option<T>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiError {
    code: i32,
    message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UsageResultData {
    end: Option<String>,

    #[serde(rename = "payloadSize")]
    payload_size: Option<String>,

    #[serde(rename = "metadataSize")]
    metadata_size: Option<String>,

    #[serde(rename = "objectCount")]
    object_count: Option<String>,

    #[serde(rename = "uploadCount")]
    upload_count: Option<String>,

    #[serde(rename = "infrequentAccessPayloadSize")]
    infrequent_access_payload_size: Option<String>,

    #[serde(rename = "infrequentAccessMetadataSize")]
    infrequent_access_metadata_size: Option<String>,

    #[serde(rename = "infrequentAccessObjectCount")]
    infrequent_access_object_count: Option<String>,

    #[serde(rename = "infrequentAccessUploadCount")]
    infrequent_access_upload_count: Option<String>,
}

trait SendAndHandle {
    async fn send_and_handle(self) -> Result<String, String>;
}

impl SendAndHandle for RequestBuilder {
    async fn send_and_handle(self) -> Result<String, String> {
        let resp = self.send().await;

        match resp {
            Ok(resp) => {
                println!("{}", resp.status());
                match resp.text().await {
                    Ok(text) => {
                        Ok(text)
                    }
                    Err(e) => Err(
                        format!("Error reading response text: {e}")
                    ),
                }
            }
            Err(e) => Err(
                format!("Request error: {e}")
            ),
        }
    }
}


pub struct R2D2 {
    account_id: String,
    apikey: String,
    pub bucket: String,
    // key id and secret?
}

impl R2D2 {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        if let Some(config) = read_configfile(path) {
            Ok(
                Self {
                    account_id: get_from_config(&config, "R2_ACCOUNT_ID")?,
                    apikey: get_from_config(&config, "R2_API_KEY")?,
                    bucket: get_from_config(&config, "R2_BUCKET")?,
                }
            )
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
        // todo: more flexible etc.

        Self::from_filename(".r2")
    }

    fn _base_url(&self) -> String {
        format!("https://api.cloudflare.com/client/v4/accounts/{}/r2/buckets/{}/", self.account_id, self.bucket)
    }

    pub fn build_url(&self, endpoint: &str) -> Option<Url> {
        let base = Url::parse(&self._base_url()).ok()?;
        base.join(endpoint).ok()
    }

    pub fn request(&self, endpoint: &str) -> Option<RequestBuilder> {
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

    pub async fn usage(&self) -> Result<ApiResponse<UsageResultData>, String> {
        // todo: parse response

        let Some(request) = self.request("usage?=null") else {
            return Err(format!("Request for '{}' could not be set up.", "usage"));
        };

        let text = request.send_and_handle().await?;

        let result: ApiResponse<UsageResultData> = serde_json::from_str(&text).map_err_to_string()?;

        Ok(result)
    }
}
