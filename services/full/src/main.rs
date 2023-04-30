use tardis::{basic::result::TardisResult, tokio};

#[tokio::main]
async fn main() -> TardisResult<()> {
    spacegate_kernel::startup(true, None, None).await?;
    Ok(())
}
