/// Reads events from state chain
pub mod sc_observer;

/// Submits events to state chain
pub mod sc_broadcaster;

/// The state chain runtime client type definitions
pub mod runtime;

mod sc_event;

// TODO: Ensure all references to the state chain are general
// As this was previously called sc-observer, but now will contain
// both the observer and the broadcaster

// ==== Pallet support for the state chain =====

/// Staking pallet support for substrate-subxt
pub mod staking;
/// Validator pallet support for substrate-subxt
pub mod validator;


// ============ Helper methods ==============

/// Create a substrate subxt client over the StateChainRuntime
async fn create_subxt_client(
    subxt_settings: settings::StateChain,
) -> Result<Client<StateChainRuntime>> {
    let client = ClientBuilder::<StateChainRuntime>::new()
        .set_url(format!(
            "ws://{}:{}",
            subxt_settings.hostname, subxt_settings.port
        ))
        .build()
        .await?;

    Ok(client)
}