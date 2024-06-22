use pyo3::{prelude as pyo, pyclass, pymethods, PyAny, PyResult, Python};

use crate::helpers::{future_pyresult_to_py, sotoi, UnwrapIntoPythonError};
use crate::r2::{UsageResultData, R2D2};

#[pyclass(module = "r2_d2")]
#[derive(Debug)]
pub struct R2Usage {
    #[pyo3(get)]
    pub end: String,
    #[pyo3(get)]
    pub payload_size: i32,
    #[pyo3(get)]
    pub metadata_size: i32,
    #[pyo3(get)]
    pub object_count: i32,
    #[pyo3(get)]
    pub upload_count: i32,
    #[pyo3(get)]
    pub infrequent_access_payload_size: i32,
    #[pyo3(get)]
    pub infrequent_access_metadata_size: i32,
    #[pyo3(get)]
    pub infrequent_access_object_count: i32,
    #[pyo3(get)]
    pub infrequent_access_upload_count: i32,
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

    let usage = r2d2.usage().await?;
    let usage_py: R2Usage = usage.into();

    Ok(usage_py)
}

#[pyo::pyfunction]
pub fn usage(py: Python<'_>) -> PyResult<&PyAny> {
    let future = usage_async();

    future_pyresult_to_py(py, future)
}
