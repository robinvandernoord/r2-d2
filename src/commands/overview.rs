use crate::cli::{OverviewOptions, Process};
use crate::commands::usage::gather_usage_info;
use crate::helpers::print_table;
use crate::r2::R2D2;

impl Process for OverviewOptions {
    async fn process(self) -> anyhow::Result<i32> {
        let r2 = R2D2::guess()?;
        let rows = gather_usage_info(&r2).await?;
        print_table(&rows);

        Ok(0)
    }
}
