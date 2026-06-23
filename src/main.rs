#[tokio::main]
async fn main() -> anyhow::Result<()> {
    summit_rcm::init_logger();
    summit_rcm::run().await
}
