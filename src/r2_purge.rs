use opendal::Operator;

use crate::r2::R2D2;

async fn empty_repo_with_opendal(op: Operator) -> anyhow::Result<()> {
    // Recursively delete everything in the bucket
    op.remove_all("").await?;

    Ok(())
}

pub async fn empty_repo(r2: &R2D2) -> anyhow::Result<()> {
    let op = r2.clone().into_opendal_operator()?;

    empty_repo_with_opendal(op).await
}
