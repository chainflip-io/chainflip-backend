use chainflip::relayer::{self, EthEventStreamBuilder, Result, StakeManager};
use relayer::sinks::{Logger, StakeManagerRuntimeCaller};

#[async_std::main]
async fn main() -> Result<()> {
    env_logger::init();

    log::debug!("Connecting to event source and sinks...");

    let event_source = StakeManager::load()?;

    let relayer = EthEventStreamBuilder::new("ws://host.docker.internal:8545", event_source)
        .with_sink(Logger::new(log::Level::Info))
        .with_sink(StakeManagerRuntimeCaller::new("ws://host.docker.internal:55029").await?)
        .build()
        .await?;

    log::info!("Starting relayer.");

    relayer.run(Some(0)).await?;

    log::info!("Relayer exiting.");

    Ok(())
}
