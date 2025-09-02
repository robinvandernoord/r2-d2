use crate::cli::{AuthOptions, Process};
use crate::helpers::IntoPythonError;
use crate::r2::R2D2;
use anyhow::bail;

impl Process for AuthOptions {
    async fn process(self) -> anyhow::Result<i32> {
        let r2d2 = R2D2::guess().to_python_error("env")?;

        if self.show {
            eprintln!("{:}", &r2d2);
        }

        let verification = r2d2.verify_py().await?;

        // Obfuscate the verification ID
        let id = &verification.id;
        let obfuscated_id = if !self.show && id.len() > 8 {
            format!("{}...{}", &id[..4], &id[id.len() - 4..])
        } else {
            id.clone()
        };

        if verification.ok() {
            println!("Authorization ok: {obfuscated_id}");
            Ok(0)
        } else {
            println!(
                "Authorization failed: {} (status: {})",
                obfuscated_id, verification.status
            );
            bail!("Authorization failed.")
        }
    }
}
