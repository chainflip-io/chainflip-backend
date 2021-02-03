use chainflip::relayer::{self, EthEventStreamer, Result, StakeManager};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let event_source = StakeManager::load()?;

    let relayer = EthEventStreamer::new(
        "ws://host.docker.internal:8545",
        event_source,
        relayer::sinks::Logger,
    )
    .await?;

    relayer.run(Some(0)).await?;

    Ok(())
}
