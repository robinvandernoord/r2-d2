use crate::r2::R2D2;
use pyo3::exceptions::PyValueError;
use pyo3::{prelude as pyo, pyclass, pymethods, IntoPy, Python};
use pyo3_asyncio::tokio::future_into_py;

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

pub async fn usage_async() -> pyo::PyResult<R2Usage> {
    // Err(PyValueError::new_err("Something went wrong mate"))

    Ok(
        R2Usage {
            end: None,
            payload_size: 0,
            metadata_size: 0,
            object_count: 0,
            upload_count: 0,
            infrequent_access_payload_size: 0,
            infrequent_access_metadata_size: 0,
            infrequent_access_object_count: 0,
            infrequent_access_upload_count: 0,
        }
    )
}

#[pyo::pyfunction]
pub fn usage(py: Python<'_>) -> pyo::PyResult<&pyo::PyAny> {
    future_into_py(py, async move {
        match usage_async().await {
            Ok(obj) => Python::with_gil(|_py| Ok(obj)),
            Err(exc) => Python::with_gil(|_py| Err(exc)),
        }
    })
}

#[pyo::pyfunction]
pub fn main_rs(py: pyo::Python<'_>) -> pyo::PyResult<&pyo::PyAny> {
    pyo3_asyncio::tokio::future_into_py(py, async {
        let exit_code = match async_main_rs().await {
            Ok(code) => code,
            Err(msg) => {
                eprintln!("{msg}");
                1
            },
        };
        Ok(pyo::Python::with_gil(|_| exit_code))
    })
}

#[pyo::pymodule]
pub fn r2_d2(
    _py: pyo::Python,
    m: &pyo::PyModule,
) -> pyo::PyResult<()> {
    m.add_class::<R2Usage>()?;

    m.add_function(pyo::wrap_pyfunction!(main_rs, m)?)?;
    m.add_function(pyo::wrap_pyfunction!(usage, m)?)?;
    Ok(())
}
