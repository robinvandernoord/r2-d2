use std::future::Future;
use std::path::PathBuf;
use std::str::FromStr;

use pyo3::exceptions::PyRuntimeError;
use pyo3::{IntoPy, PyAny, PyErr, PyObject, PyResult, Python};

/// ASCII to Integer
pub fn atoi(ascii: &str) -> i64 {
    i64::from_str(ascii).unwrap_or_default()
}

/// String to Integer
#[allow(clippy::needless_pass_by_value)]
pub fn stoi(ascii: String) -> i64 {
    atoi(&ascii)
}

/// ASCII Option to Integer
pub fn aotoi(ascii_option: Option<&str>) -> i64 {
    atoi(ascii_option.unwrap_or_default())
}

/// String Option to Integer
pub fn sotoi(ascii_option: Option<String>) -> i64 {
    stoi(ascii_option.unwrap_or_default())
}

pub trait ResultToString<T, E> {
    fn map_err_to_string(self) -> Result<T, String>;
}

impl<T, E: std::error::Error> ResultToString<T, E> for Result<T, E> {
    fn map_err_to_string(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}

// https://users.rust-lang.org/t/is-there-a-simple-way-to-give-a-default-string-if-the-string-variable-is-empty/100411

pub trait StringExt {
    fn or(
        self,
        dflt: &str,
    ) -> String;
}

impl<S: Into<String>> StringExt for S {
    fn or(
        self,
        dflt: &str,
    ) -> String {
        // Re-use a `String`s capacity, maybe
        let mut s = self.into();
        if s.is_empty() {
            s.push_str(dflt);
        }
        s
    }
}

pub trait PathToString {
    fn to_string(self) -> String;
}

impl PathToString for PathBuf {
    fn to_string(self) -> String {
        self.into_os_string().into_string().unwrap_or_default()
    }
}

pub fn future_pyresult_to_py<
    T: IntoPy<PyObject> + Send + 'static,
    E: Into<PyErr> + Send + 'static,
>(
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

pub trait IntoPythonError<T> {
    fn to_python_error(
        self,
        hint: &str,
    ) -> PyResult<T>;
}

pub trait UnwrapIntoPythonError<T> {
    fn unwrap_or_raise(self) -> PyResult<T>;
}

impl<T> IntoPythonError<T> for Result<T, String> {
    fn to_python_error(
        self,
        _: &str,
    ) -> PyResult<T> {
        self.map_err(PyRuntimeError::new_err)
    }
}

impl<T> UnwrapIntoPythonError<T> for Result<T, String> {
    fn unwrap_or_raise(self) -> PyResult<T> {
        self.map_err(PyRuntimeError::new_err)
    }
}
