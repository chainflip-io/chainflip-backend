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
    log_key: &'static str,
    state_chain_client: Arc<StateChainClient<StateChainRpc>>,
    mut epoch_start_receiver: broadcast::Receiver<EpochStart>,
    mut should_participant_witness_epoch: G,
    initial_state: State,
    mut epoch_witnesser_generator: F,
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
                let logger = logger.new(o!(COMPONENT_KEY => format!("{}-Witnesser", log_key)));
                slog::info!(&logger, "Starting");

                let mut option_state = Some(initial_state);
                let mut end_witnessing_signal_and_handle: Option<(
                    Arc<Mutex<Option<u64>>>,
                    ScopedJoinHandle<State>,
                )> = None;

                loop {
                    let epoch_start = epoch_start_receiver.recv().await?;

                    if let Some((end_witnessing_signal, handle)) =
                        end_witnessing_signal_and_handle.take()
                    {
                        *end_witnessing_signal.lock().unwrap() = Some(epoch_start.eth_block);
                        option_state = Some(handle.await);
                    }

                    if epoch_start.participant && should_participant_witness_epoch(&epoch_start) {
                        end_witnessing_signal_and_handle = Some({
                            let end_witnessing_signal = Arc::new(Mutex::new(None));

                            let state_chain_client = state_chain_client.clone();
                            let logger = logger.clone();

                            (
                                end_witnessing_signal.clone(),
                                scope.spawn_with_handle::<_, _>(epoch_witnesser_generator(
                                    state_chain_client,
                                    end_witnessing_signal,
                                    epoch_start,
                                    option_state.take().unwrap(),
                                    logger,
                                )),
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
