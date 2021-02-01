use chainflip::relayer::{self, EthEventStreamer, Result, StakeManager};

#[tokio::main]
async fn main() -> Result<()> {
    let event_source = StakeManager::load()?;

    let relayer =
        EthEventStreamer::new("ws://localhost::8545", event_source, relayer::sinks::Stdout).await?;

    relayer.run(None).await?;

    Ok(())
}
