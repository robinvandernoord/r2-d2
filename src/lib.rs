use pyo3::prelude as pyo;
use crate::r2::R2D2;

pub mod r2;
pub mod helpers;

async fn async_main_rs() -> Result<i32, String> {
    let r2d2 = R2D2::from_dot_r2()?;

    let response = r2d2.usage().await?;

    println!("{response:?}");
    Ok(0)
}


#[pyo::pyfunction]
/// # Errors
/// should be handled gracefully?
pub fn main_rs(py: pyo::Python<'_>) -> pyo::PyResult<&pyo::PyAny> {
    pyo3_asyncio::tokio::future_into_py(py, async {
        let exit_code = match async_main_rs().await {
            Ok(code) => { code }
            Err(msg) => {
                eprintln!("{msg}");
                1
            }
        };
        Ok(pyo::Python::with_gil(|_| exit_code))
    })
}

#[pyo::pymodule]
/// # Errors
/// should be handled gracefully?
pub fn r2_d2(_py: pyo::Python, m: &pyo::PyModule) -> pyo::PyResult<()> {
    m.add_function(pyo::wrap_pyfunction!(main_rs, m)?)?;
    Ok(())
}