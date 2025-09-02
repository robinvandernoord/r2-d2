use anyhow::{Context, bail};
use futures::StreamExt;
use opendal::Operator;
use rustic_core::Progress;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

use crate::r2::R2D2;
use crate::rustic_progress::ProgressBar;

async fn empty_repo_with_opendal(op: Operator) -> anyhow::Result<()> {
    // Recursively delete everything in the bucket
    op.remove_all("").await?;

    Ok(())
}


pub async fn empty_repo(
    r2: &R2D2,
) -> anyhow::Result<()> {
    let op = r2.clone().into_opendal_operator()?;

    empty_repo_with_opendal(op).await
}
