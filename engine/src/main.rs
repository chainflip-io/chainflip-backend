#[tokio::main]
async fn main() -> anyhow::Result<()> {
	chainflip_engine::start().await
}
