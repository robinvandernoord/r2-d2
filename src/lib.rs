use std::collections::BTreeMap;
use std::path::Path;

use dotenvy::from_path_iter;
use pyo3::prelude as pyo;
use reqwest::Client;
use reqwest::header::HeaderMap;

struct R2D2Config {
    account_id: String,
    apikey: String,
    bucket: String,
    // key id and secret?
}

fn get_from_config(
    config: &BTreeMap<String, String>,
    key: &str,
) -> Result<String, String> {
    config.get(key).map_or_else(|| Err(format!("Key {key} could not be found in the config.")), |value| Ok(value.clone()))
}

impl R2D2Config {
    /// Read .r2 config file
    pub fn from_dot_r2() -> Result<Self, String> {
        // todo: more flexible etc.

        if let Some(config) = read_configfile(".r2") {
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

    pub fn base_url(&self) -> String {
        format!("https://api.cloudflare.com/client/v4/accounts/{}/r2/buckets/{}/", self.account_id, self.bucket)
    }

    pub fn headers(&self) -> Option<HeaderMap> {
        let mut headers = HeaderMap::new();
        let bearer = format!("Bearer {}", self.apikey);
        headers.insert("Authorization", bearer.parse().ok()?);

        Some(headers)
    }
}


fn read_configfile(filename: &str) -> Option<BTreeMap<String, String>> {
    let path = Path::new(filename);
    let iter = from_path_iter(path).ok()?;

    let mut config: BTreeMap<String, String> = BTreeMap::new();

    for item in iter {
        let (key, value) = item.ok()?;
        config.insert(key, value);
    }

    Some(config)
}


async fn async_main_rs() -> Result<i32, String> {
    let r2d2 = R2D2Config::from_dot_r2()?;
    let client = Client::new();
    let url = format!("{}/usage?=null", r2d2.base_url());
    let mut request = client.get(url);
    if let Some(headers) = r2d2.headers() {
        request = request.headers(headers);
    }

    match request.send().await {
        Ok(resp) => {
            println!("{}", resp.status());
            match resp.text().await {
                Ok(text) => {
                    println!("{text}");
                    Ok(0)
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

#[pyo::pyfunction]
/// # Errors
/// should be handled gracefully?
pub fn main_rs(py: pyo::Python<'_>) -> pyo::PyResult<&pyo::PyAny> {
    pyo3_asyncio::tokio::future_into_py(py, async {
        let exit_code = match async_main_rs().await {
            Ok(code) => {code}
            Err(msg) => {
                eprintln!("{msg}");
                1
            }
        };

        Ok(pyo::Python::with_gil(|_| exit_code))
    })
}

#[pyo::pymodule]
/// # Errors
/// should be handled gracefully?
pub fn r2_d2(_py: pyo::Python, m: &pyo::PyModule) -> pyo::PyResult<()> {
    m.add_function(pyo::wrap_pyfunction!(main_rs, m)?)?;
    Ok(())
}