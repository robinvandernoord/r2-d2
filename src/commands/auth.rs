use crate::cli::{AuthOptions, Process};
use crate::helpers::IntoPythonError;
use crate::r2::R2D2;
use anyhow::bail;

impl Process for AuthOptions {
    async fn process(self) -> anyhow::Result<i32> {
        let r2d2 = R2D2::guess().to_python_error("env")?;

        let verification = r2d2.verify_py().await?;

        if verification.ok() {
            println!("Authorization ok!");
            Ok(0)
        } else {
            bail!("Authorization failed.")
        }
    }
}
