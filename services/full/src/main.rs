use tardis::{basic::result::TardisResult, tokio};

#[tokio::main]
async fn main() -> TardisResult<()> {
    let namespaces = std::env::args().nth(1).expect("The first parameter is missing: kubernetes namespaces");
    spacegate_kernel::startup(true, namespaces, None).await?;
    Ok(())
}
