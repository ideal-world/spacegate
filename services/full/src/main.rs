use tardis::{basic::result::TardisResult, tokio};

#[tokio::main]
async fn main() -> TardisResult<()> {
    let namespaces = std::env::args().nth(1).map(Some).unwrap_or(None);
    spacegate_kernel::startup(true, namespaces, None).await?;
    Ok(())
}
