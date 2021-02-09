use chainflip::relayer::{self, EthEventStreamBuilder, Result, StakeManager};
use relayer::sinks::{Logger, StakingCall, StateChainCaller};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    log::debug!("Connecting to event source and sinks...");

    let event_source = StakeManager::load()?;

    let relayer = EthEventStreamBuilder::new("ws://host.docker.internal:8545", event_source)
        .with_sink(Logger::default())
        .with_sink(Logger::new(log::Level::Info))
        .with_sink(StateChainCaller::<StakingCall>::new("http://host.docker.internal:9944").await?)
        .build()
        .await?;

    log::info!("Starting relayer.");

    relayer.run(Some(0)).await?;

    log::info!("Relayer exiting.");

    Ok(())
}
