mod relayer;

use relayer::{
    contracts::stake_manager::StakeManager,
    sinks::{Logger, StateChainCaller},
    EthEventStreamBuilder, Result,
};

#[async_std::main]
async fn main() -> Result<()> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    let state_chain_url = args[1].as_str();
    let eth_chain_url = args[2].as_str();
    let contract_address = args[3].as_str();

    log::debug!("Connecting to event source and sinks...");

    let event_source = StakeManager::load(contract_address)?;

    let state_chain = StateChainCaller::new(state_chain_url).await?;

    let relayer = EthEventStreamBuilder::new(eth_chain_url, event_source)
        .with_sink(Logger::new(log::Level::Info))
        .with_sink(state_chain)
        .build()
        .await?;

    log::info!("Starting relayer.");

    relayer.run(Some(0)).await?;

    log::info!("Relayer exiting.");

    Ok(())
}
