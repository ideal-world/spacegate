use tardis::{basic::result::TardisResult, tokio};

#[tokio::main]
async fn main() -> TardisResult<()> {
    let conf_path = std::env::args().nth(1).expect("The first parameter is missing: configuration path");
    let check_interval_sec = std::env::args().nth(2).expect("The second parameter is missing: configuration change check period (in seconds)");
    spacegate_kernel::startup(false, Some(conf_path), Some(check_interval_sec.parse().unwrap())).await?;
    Ok(())
}
