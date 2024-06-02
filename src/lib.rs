use pyo3::exceptions::PyValueError;
use pyo3::prelude::PyModule;
use pyo3::{prelude as pyo, PyAny, PyResult, Python};

use crate::commands::usage::usage;
use crate::commands::usage::R2Usage;
use crate::helpers::result_to_py;

pub mod commands;
pub mod helpers;
pub mod r2;

#[allow(clippy::unused_async)]
async fn async_main_rs() -> Result<i32, String> {
    // todo: clap
    Ok(0)
}

#[allow(clippy::unused_async)]
pub async fn error_async() -> PyResult<R2Usage> {
    Err(PyValueError::new_err("Something went wrong mate"))
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
