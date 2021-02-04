use chainflip::relayer::{self, EthEventStreamBuilder, Result, StakeManager};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let event_source = StakeManager::load()?;

    let relayer = EthEventStreamBuilder::new("ws://host.docker.internal:8545", event_source)
        .with_sink(relayer::sinks::Logger::default())
        .with_sink(relayer::sinks::Logger::new(log::Level::Info))
        .build()
        .await?;

    relayer.run(Some(0)).await?;

    Ok(())
}
