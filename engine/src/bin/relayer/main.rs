use chainflip::relayer::{EventStreamer, Result, StakeManager};

#[tokio::main]
async fn main() -> Result<()> {
    let stakingEventSource = StakeManager::load()?;

    let relayer = EventStreamer::new("ws://path/to/eth/endpoint", stakingEventSource).await?;

    // Load the contracts,

    relayer.run(None).await?;

    Ok(())
}
