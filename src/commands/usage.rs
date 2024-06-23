use crate::commands::list::ListOptions;
use byte_unit::{Byte, UnitType};
use futures::future;
use owo_colors::OwoColorize;
use pyo3::{prelude as pyo, pyclass, pymethods, PyAny, PyResult, Python};
use std::collections::BTreeMap;
use tabled::Tabled;

use crate::helpers::{future_pyresult_to_py, sotoi, ResultToString, UnwrapIntoPythonError};
use crate::r2::{UsageResultData, R2D2};

#[pyclass(module = "r2_d2")]
#[derive(Debug)]
pub struct R2Usage {
    #[pyo3(get)]
    pub end: String,
    #[pyo3(get)]
    pub payload_size: i64,
    #[pyo3(get)]
    pub metadata_size: i64,
    #[pyo3(get)]
    pub object_count: i64,
    #[pyo3(get)]
    pub upload_count: i64,
    #[pyo3(get)]
    pub infrequent_access_payload_size: i64,
    #[pyo3(get)]
    pub infrequent_access_metadata_size: i64,
    #[pyo3(get)]
    pub infrequent_access_object_count: i64,
    #[pyo3(get)]
    pub infrequent_access_upload_count: i64,
}

impl From<UsageResultData> for R2Usage {
    fn from(value: UsageResultData) -> Self {
        Self {
            end: value.end.unwrap_or_default(),
            payload_size: sotoi(value.payload_size),
            metadata_size: sotoi(value.metadata_size),
            object_count: sotoi(value.object_count),
            upload_count: sotoi(value.upload_count),
            infrequent_access_payload_size: sotoi(value.infrequent_access_payload_size),
            infrequent_access_metadata_size: sotoi(value.infrequent_access_metadata_size),
            infrequent_access_object_count: sotoi(value.infrequent_access_object_count),
            infrequent_access_upload_count: sotoi(value.infrequent_access_upload_count),
        }
    }
}

#[pymethods]
impl R2Usage {
    pub fn __str__(&self) -> String {
        format!("<r2_d2.R2Usage object at {self:p}>")
    }
    pub fn __repr__(&self) -> String {
        format!("<{self:#?}>")
    }
}

#[allow(clippy::unused_async)]
pub async fn usage_async() -> PyResult<R2Usage> {
    let r2d2 = R2D2::guess().unwrap_or_raise()?;

    let usage = r2d2.usage(None).await?;
    let usage_py: R2Usage = usage.into();

    Ok(usage_py)
}

#[pyo::pyfunction]
pub fn usage(py: Python<'_>) -> PyResult<&PyAny> {
    let future = usage_async();

    future_pyresult_to_py(py, future)
}

#[derive(Tabled)]
pub struct UsageTable {
    bucket_name: String,
    raw_size: i64,
    human_size: String,
}

impl UsageTable {
    pub fn new<S: Into<String>>(
        name: S,
        size: i64,
    ) -> Self {
        let byte = Byte::from_i64(size)
            .unwrap_or_default()
            .get_appropriate_unit(UnitType::Decimal);
        Self {
            bucket_name: name.into(),
            raw_size: size,
            human_size: format!("{byte:#.2}"),
        }
    }

    pub fn bold(mut self) -> Self {
        self.bucket_name = self.bucket_name.bold().to_string();
        self.human_size = self.human_size.bold().to_string();

        self
    }
}
fn calculate_sum(rows: &[UsageTable]) -> i64 {
    rows.iter().fold(0, |sum, row| sum + row.raw_size)
}

pub async fn gather_usage_info(r2: &R2D2) -> Result<Vec<UsageTable>, String> {
    // todo: move elsewhere
    // 1. list buckets
    // 2. gather usage data
    // 3. return table (str)
    let buckets = r2
        .list(Some(ListOptions::default()))
        .await
        .map_err_to_string()?;

    let bucket_names: Vec<String> = buckets.into_iter().map(|bucket| bucket.name).collect();

    let promises: Vec<_> = bucket_names
        .iter()
        .map(|bucket_name| r2.usage(Some(bucket_name.clone())))
        .collect();

    let results = future::join_all(promises).await;

    let map: BTreeMap<_, R2Usage> = bucket_names
        .into_iter()
        .zip(results)
        // only keep Ok values, turn from UsageResultData into R2Usage
        .filter_map(|(key, value)| value.map_or(None, |value| Some((key, value.into()))))
        .collect();

    let mut rows: Vec<UsageTable> = map
        .iter()
        .map(|(k, v)| UsageTable::new(k.clone(), v.payload_size))
        .collect();

    // footer:
    rows.push(UsageTable::new("total", calculate_sum(&rows)).bold());

    Ok(rows)
}
