use std::sync::Arc;

use futures::{future::Fuse, Future, FutureExt};
use tokio::sync::{oneshot, Mutex};
use tracing::info;

use crate::task_scope::{task_scope, ScopedJoinHandle};

use super::{ChainBlockNumber, EpochStart};

pub async fn start<G, F, Fut, FutErr, State, Chain>(
	epoch_start_receiver: Arc<Mutex<async_broadcast::Receiver<EpochStart<Chain>>>>,
	mut should_epoch_participant_witness: G,
	initial_state: State,
	mut epoch_witnesser_generator: F,
) -> Result<(), FutErr>
where
	Chain: cf_chains::Chain,
	F: FnMut(Fuse<oneshot::Receiver<ChainBlockNumber<Chain>>>, EpochStart<Chain>, State) -> Fut
		+ Send
		+ 'static,
	Fut: Future<Output = Result<State, FutErr>> + Send + 'static,
	FutErr: Send + 'static,
	State: Send + 'static,
	G: FnMut(&EpochStart<Chain>) -> bool + Send + 'static,
{
	task_scope(|scope| {
		{
			async {
				info!("Starting");

				let mut option_state = Some(initial_state);
				let mut end_witnessing_channel_and_handle: Option<(
					oneshot::Sender<ChainBlockNumber<Chain>>,
					ScopedJoinHandle<State>,
				)> = None;

				loop {
					let epoch_start =
						epoch_start_receiver.lock().await.recv().await.expect("Sender closed");
					let (end_witnessing_sender, end_witnessing_receiver) = oneshot::channel();

					// Send a signal to the previous epoch to stop at the starting block of the new
					// epoch
					if let Some((end_prev_epoch_sender, handle)) =
						end_witnessing_channel_and_handle.take()
					{
						let _res = end_prev_epoch_sender.send(epoch_start.block_number);
						option_state = Some(handle.await);
					}

					if epoch_start.participant && should_epoch_participant_witness(&epoch_start) {
						info!("Start witnessing from block: {}", epoch_start.block_number);

						end_witnessing_channel_and_handle = Some((
							end_witnessing_sender,
							scope.spawn_with_handle::<_, _>(epoch_witnesser_generator(
								end_witnessing_receiver.fuse(),
								epoch_start,
								option_state.take().unwrap(),
							)),
						));
					}
				}
			}
		}
		.boxed()
	})
	.await
}
