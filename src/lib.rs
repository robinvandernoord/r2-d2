use pyo3::exceptions::PyValueError;
use pyo3::prelude::PyModule;
use pyo3::{prelude as pyo, PyAny, PyResult, Python};

use crate::commands::usage::usage;
use crate::commands::usage::R2Usage;
use crate::helpers::future_pyresult_to_py;
use crate::r2::R2D2;
use crate::r2_upload::upload_example;

pub mod commands;
pub mod helpers;
pub mod r2;
mod r2_upload;

#[allow(clippy::unused_async)]
async fn async_main_rs() -> Result<i32, String> {
    let r2 = R2D2::guess()?;

    // todo: clap
    // subcommand 'overview':
    // let rows = gather_usage_info(&r2).await?;
    // print_table(&rows);

    // subcommand 'upload':
    // upload_example(r2, "/home/robin/Downloads/sport.vst".to_string(), None).await?;
    upload_example(r2, "/home/robin/Downloads/sport.zip".to_string(), None).await?;
    // upload_example(r2, "/home/robin/Downloads/article.image.860b6373d9dac5f1.aW1hZ2UwMDAxMS5qcGVn.jpeg".to_string(), None).await?;

    Ok(0)
}

#[allow(clippy::unused_async)]
pub async fn error_async() -> PyResult<R2Usage> {
    Err(PyValueError::new_err("Something went wrong mate"))
}

#[pyo::pyfunction]
pub fn error(py: Python<'_>) -> PyResult<&PyAny> {
    let future = error_async();

    future_pyresult_to_py(py, future)
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
