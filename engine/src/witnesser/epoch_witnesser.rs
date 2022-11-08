use std::sync::{Arc, Mutex};

use futures::{Future, FutureExt};
use slog::o;

use crate::{
	logging::COMPONENT_KEY,
	task_scope::{with_task_scope, ScopedJoinHandle},
};

use super::{ChainBlockNumber, EpochStart};

pub fn should_end_witnessing<Chain: cf_chains::Chain>(
	end_witnessing_signal: Arc<Mutex<Option<ChainBlockNumber<Chain>>>>,
	current_block_number: ChainBlockNumber<Chain>,
	logger: &slog::Logger,
) -> bool {
	if let Some(end_block) = *end_witnessing_signal.lock().unwrap() {
		if current_block_number >= end_block {
			slog::info!(
				logger,
				"Finished witnessing events at ETH block: {}",
				current_block_number
			);
			// we have reached the block height we wanted to witness up to
			// so can stop the witness process
			return true
		}
	}
	false
}

pub async fn start<G, F, Fut, State, Chain>(
	log_key: String,
	epoch_start_receiver: async_channel::Receiver<EpochStart<Chain>>,
	mut should_epoch_participant_witness: G,
	initial_state: State,
	mut epoch_witnesser_generator: F,
	logger: &slog::Logger,
) -> anyhow::Result<()>
where
	Chain: cf_chains::Chain,
	F: FnMut(
			Arc<Mutex<Option<ChainBlockNumber<Chain>>>>,
			EpochStart<Chain>,
			State,
			slog::Logger,
		) -> Fut
		+ Send
		+ 'static,
	Fut: Future<Output = anyhow::Result<State>> + Send + 'static,
	State: Send + 'static,
	G: FnMut(&EpochStart<Chain>) -> bool + Send + 'static,
{
	with_task_scope(|scope| {
		{
			async {
				let logger = logger.new(o!(COMPONENT_KEY => format!("{}-Witnesser", log_key)));
				slog::info!(&logger, "Starting");

				let mut option_state = Some(initial_state);
				let mut end_witnessing_signal_and_handle: Option<(
					Arc<Mutex<Option<ChainBlockNumber<Chain>>>>,
					ScopedJoinHandle<State>,
				)> = None;

				loop {
					let epoch_start = epoch_start_receiver.recv().await?;

					if let Some((end_witnessing_signal, handle)) =
						end_witnessing_signal_and_handle.take()
					{
						*end_witnessing_signal.lock().unwrap() = Some(epoch_start.block_number);
						option_state = Some(handle.await);
					}

					if epoch_start.participant && should_epoch_participant_witness(&epoch_start) {
						end_witnessing_signal_and_handle = Some({
							let end_witnessing_signal = Arc::new(Mutex::new(None));

							let logger = logger.clone();

							slog::info!(
								logger,
								"Start witnessing from ETH block: {}",
								epoch_start.block_number
							);

							(
								end_witnessing_signal.clone(),
								scope.spawn_with_handle::<_, _>(epoch_witnesser_generator(
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
