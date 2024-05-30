use std::collections::BTreeMap;
use std::env;
use std::path::Path;
use std::process::exit;

use dotenvy::from_path_iter;
use pyo3::{Bound, pyfunction, pymodule, PyResult, Python, wrap_pyfunction};
use pyo3::prelude::{PyAnyMethods, PyModule};
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

async fn _main() -> Result<i32, String> {
    let r2d2 = R2D2Config::from_dot_r2()?;
    let client = Client::new();
    let url = format!("{}/usage?=null", r2d2.base_url());
    let mut request = client.get(url);
    if let Some(headers) = r2d2.headers() {
        request = request.headers(headers);
    }

    match request.send().await {
        Ok(resp) => {
            println!("{}", resp.status().as_u16());
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

#[pyfunction]
fn python_main(py: Python) -> PyResult<()> {
    // This one includes python and the name of the wrapper script itself, e.g.
    // `["/home/ferris/.venv/bin/python", "/home/ferris/.venv/bin/print_cli_args", "a", "b", "c"]`
    println!("{:?}", env::args().collect::<Vec<_>>());
    // This one includes only the name of the wrapper script itself, e.g.
    // `["/home/ferris/.venv/bin/print_cli_args", "a", "b", "c"])`
    println!(
        "{:?}",
        py.import_bound("sys")?
            .getattr("argv")?
            .extract::<Vec<String>>()?
    );
    Ok(())
}

#[pymodule]
fn r2_d2(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_wrapped(wrap_pyfunction!(python_main))?;

    Ok(())
}

// #[tokio::main]
// async fn main() {
//     match _main().await {
//         Ok(code) => {
//             exit(code)
//         }
//         Err(msg) => {
//             eprintln!("{msg}");
//             exit(1)
//         }
//     }
// }