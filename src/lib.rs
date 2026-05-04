use crate::cli::{Args, Process};
use crate::commands::usage::R2Usage;
use crate::commands::usage::usage;
use crate::helpers::{UnwrapIntoPythonError, fmt_error, future_pyresult_to_py};
use clap::{Command, CommandFactory, Parser};
use clap_complete::{Generator, generate};
use futures::FutureExt;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::PyModule;
use pyo3::{IntoPy, PyAny, PyObject, PyResult, Python, prelude as pyo};
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::process::exit;
use std::{env, io};

pub mod cli;
pub mod commands;
pub mod helpers;
pub mod r2;
pub mod r2_purge;

pub mod r2_upload;

pub mod rustic_backends;
mod rustic_progress;
mod config;

pub fn future_into_py_catch_panic<
    F: Future<Output = PyResult<T>> + Send + 'static,
    T: Send + 'static + IntoPy<PyObject>,
>(
    py: Python<'_>,
    fut: F,
) -> PyResult<&PyAny> {
    pyo3_asyncio::tokio::future_into_py(py, async move {
        match AssertUnwindSafe(fut).catch_unwind().await {
            Ok(py_res) => py_res,
            Err(panic) => {
                let msg = panic.downcast_ref::<&'static str>().map_or_else(
                    || {
                        panic
                            .downcast_ref::<String>()
                            .map_or("unknown panic", |s| s.as_str())
                    },
                    |s| *s,
                );

                Err(PyRuntimeError::new_err(format!(
                    "rust future panicked: {msg}"
                )))
            },
        }
    })
}

pub fn print_completions<G: Generator>(
    generator: G,
    cmd: &mut Command,
) {
    // get_name returns a str, to_owned = to_string (but restriction::str_to_string)
    generate(generator, cmd, cmd.get_name().to_owned(), &mut io::stdout());
}

async fn async_main_rs() -> anyhow::Result<i32> {
    // skip first (`python r2d2 subcommand` -> `r2d2 subcommand`):

    let args = Args::parse_from(env::args().skip(1));

    let exit_code = if let Some(generator) = args.generator {
        let mut cmd = Args::command();

        print_completions(generator, &mut cmd);
        0
    } else {
        args.cmd.process().await.unwrap_or_else(|msg| {
            eprintln!("{}", fmt_error(&msg));
            1
        })
    };

    exit(exit_code);
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
    future_into_py_catch_panic(py, async { async_main_rs().await.unwrap_or_raise() })
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
