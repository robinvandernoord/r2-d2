use crate::helpers::{result_to_py, IntoPythonError, UnwrapIntoPythonError};
use crate::r2::R2D2;
use pyo3::{prelude as pyo, pyclass, pymethods, PyAny, PyResult, Python};

#[pyclass(module = "r2_d2")]
#[derive(Debug)]
pub struct R2Usage {
    #[pyo3(get)]
    pub end: Option<String>,
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
    let r2d2 = R2D2::from_dot_r2().unwrap_or_raise()?;

    let response = r2d2.usage().await.unwrap_or_raise()?;
    response.to_python_error("usage")?;

    println!("{response:?}");

    Ok(R2Usage {
        end: None,
        payload_size: 0,
        metadata_size: 0,
        object_count: 0,
        upload_count: 0,
        infrequent_access_payload_size: 0,
        infrequent_access_metadata_size: 0,
        infrequent_access_object_count: 0,
        infrequent_access_upload_count: 0,
    })
}

#[pyo::pyfunction]
pub fn usage(py: Python<'_>) -> PyResult<&PyAny> {
    let future = usage_async();

    result_to_py(py, future)
}
