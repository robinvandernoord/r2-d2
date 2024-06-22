use crate::commands::usage::usage;
use crate::commands::usage::R2Usage;
use crate::helpers::{future_pyresult_to_py, ResultToString};
use crate::r2::{ListOptions, R2D2};
use byte_unit::{Byte, UnitType};
use futures::future;
use owo_colors::OwoColorize;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::PyModule;
use pyo3::{prelude as pyo, PyAny, PyResult, Python};
use std::collections::BTreeMap;
use tabled::settings as table;
use tabled::settings::object::{Columns, Rows};
use tabled::{Table, Tabled};

pub mod commands;
pub mod helpers;
pub mod r2;

#[derive(Tabled)]
struct UsageTable {
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

pub fn print_table<T: Tabled>(rows: &Vec<T>) {
    let table_config = table::Settings::default()
        .with(table::Style::rounded())
        // .with(table::Padding::new(2, 2, 1, 1))
        ;
    let mut table = Table::new(rows);
    table.with(table_config);
    table.modify(Columns::last(), table::Alignment::right());
    table.with(table::themes::Colorization::exact(
        [table::Color::BG_BRIGHT_BLACK],
        Rows::last(),
    ));

    println!("{table}");
}

fn calculate_sum(rows: &[UsageTable]) -> i64 {
    rows.iter().fold(0, |sum, row| sum + row.raw_size)
}

async fn gather_usage_info(r2: &R2D2) -> Result<Vec<UsageTable>, String> {
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

#[allow(clippy::unused_async)]
async fn async_main_rs() -> Result<i32, String> {
    let r2 = R2D2::guess()?;

    // todo: clap
    // subcommand 'overview':
    let rows = gather_usage_info(&r2).await?;
    print_table(&rows);

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
