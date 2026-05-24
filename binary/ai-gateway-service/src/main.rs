#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ai_gateway_service::app::run().await
}
