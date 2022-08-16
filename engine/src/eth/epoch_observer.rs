use std::sync::{Arc, Mutex};

use futures::{Future, FutureExt};
use slog::o;
use tokio::sync::broadcast;

use crate::{
    logging::COMPONENT_KEY,
    state_chain_observer::client::{StateChainClient, StateChainRpcApi},
    task_scope::{with_task_scope, ScopedJoinHandle},
};

use super::EpochStart;

pub async fn start<G, F, Fut, State, StateChainRpc>(
    name: String,
    state_chain_client: Arc<StateChainClient<StateChainRpc>>,
    mut epoch_start_receiver: broadcast::Receiver<EpochStart>,
    mut observer_condition: G,
    initial_state: State,
    mut spawn_epoch_observer: F,
    logger: &slog::Logger,
) -> anyhow::Result<()>
where
    StateChainRpc: 'static + StateChainRpcApi + Sync + Send,
    F: FnMut(
            Arc<StateChainClient<StateChainRpc>>,
            Arc<Mutex<Option<u64>>>,
            EpochStart,
            State,
            slog::Logger,
        ) -> Fut
        + Send
        + 'static,
    Fut: Future<Output = anyhow::Result<State>> + Send + 'static,
    State: Send + 'static,
    G: FnMut(&EpochStart) -> bool + Send + 'static,
{
    with_task_scope(|scope| {
        {
            async {
                let logger = logger.new(o!(COMPONENT_KEY => name));
                slog::info!(&logger, "Starting");

                let mut option_state = Some(initial_state);
                let mut handle_and_end_observation_signal: Option<(
                    ScopedJoinHandle<State>,
                    Arc<Mutex<Option<u64>>>,
                )> = None;

                loop {
                    let epoch_start = epoch_start_receiver.recv().await?;

                    if let Some((handle, end_observation_signal)) =
                        handle_and_end_observation_signal.take()
                    {
                        *end_observation_signal.lock().unwrap() = Some(epoch_start.eth_block);
                        option_state = Some(handle.await);
                    }

                    if observer_condition(&epoch_start) {
                        handle_and_end_observation_signal = Some({
                            let end_observation_signal = Arc::new(Mutex::new(None));

                            // clone for capture by tokio task
                            let end_observation_signal_c = end_observation_signal.clone();
                            let state_chain_client = state_chain_client.clone();
                            let logger = logger.clone();

                            (
                                scope.spawn_with_handle::<_, _>(spawn_epoch_observer(
                                    state_chain_client,
                                    end_observation_signal,
                                    epoch_start,
                                    option_state.take().unwrap(),
                                    logger,
                                )),
                                end_observation_signal_c,
                            )
                        });
                    }
                }
            }
        }
        .boxed()
    })
    .await
}
