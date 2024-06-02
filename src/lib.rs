use crate::r2::R2D2;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::PyModule;
use pyo3::{prelude as pyo, pyclass, pymethods, IntoPy, PyAny, PyErr, PyObject, PyResult, Python};
use std::future::Future;

pub mod helpers;
pub mod r2;

async fn async_main_rs() -> Result<i32, String> {
    let r2d2 = R2D2::from_dot_r2()?;

    let response = r2d2.usage().await?;

    println!("{response:?}");
    Ok(0)
}

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

// todo: R2Result py enum

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
pub async fn error_async() -> PyResult<R2Usage> {
    Err(PyValueError::new_err("Something went wrong mate"))
}

#[allow(clippy::unused_async)]
pub async fn usage_async() -> PyResult<R2Usage> {
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

fn result_to_py<T: IntoPy<PyObject> + Send + 'static, E: Into<PyErr> + Send + 'static>(
    py: Python<'_>,
    future: impl Future<Output = Result<T, E>> + Send + 'static,
) -> PyResult<&'_ PyAny> {
    pyo3_asyncio::tokio::future_into_py(py, async move {
        match future.await {
            Ok(obj) => Python::with_gil(|py| Ok(obj.into_py(py))),
            Err(exc) => Python::with_gil(|_py| Err(exc.into())),
        }
    })
}

#[pyo::pyfunction]
pub fn usage(py: Python<'_>) -> PyResult<&PyAny> {
    let future = usage_async();

    result_to_py(py, future)
}

#[pyo::pyfunction]
pub fn error(py: Python<'_>) -> PyResult<&PyAny> {
    let future = error_async();

    result_to_py(py, future)
}

#[pyo::pyfunction]
pub fn main_rs(py: Python<'_>) -> PyResult<&PyAny> {
    pyo3_asyncio::tokio::future_into_py(py, async {
        let exit_code = async_main_rs().await.unwrap_or_else(|msg| {
            eprintln!("{msg}");
            1
        });

        Ok(Python::with_gil(|_| exit_code))
    })
}

#[pyo::pymodule]
pub fn r2_d2(
    _py: Python,
    m: &PyModule,
) -> PyResult<()> {
    m.add_class::<R2Usage>()?;

    m.add_function(pyo::wrap_pyfunction!(main_rs, m)?)?;
    m.add_function(pyo::wrap_pyfunction!(usage, m)?)?;
    m.add_function(pyo::wrap_pyfunction!(error, m)?)?;
    Ok(())
}
