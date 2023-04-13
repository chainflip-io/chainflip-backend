use std::sync::Arc;

use futures::{FutureExt, Stream, StreamExt};
use num_traits::CheckedSub;
use std::pin::Pin;
use tokio::{
	select,
	sync::{oneshot, Mutex},
};

use async_trait::async_trait;
use tracing::{error, info};

use crate::task_scope::{task_scope, ScopedJoinHandle};

use super::{BlockNumberable, ChainBlockNumber, EpochStart};
type BlockNumber<Witnesser> = ChainBlockNumber<<Witnesser as EpochWitnesser>::Chain>;

#[async_trait]
pub trait EpochWitnesser: Send + Sync + 'static {
	type Chain: cf_chains::Chain;
	/// Chunk of data to process in each call to [Self::do_witness]
	type Data: Send;
	/// State that persists across epochs
	type StaticState: Send;

	const SHOULD_PROCESS_HISTORICAL_EPOCHS: bool;

	async fn run_witnesser(
		self,
		mut data_stream: std::pin::Pin<
			Box<dyn futures::Stream<Item = anyhow::Result<Self::Data>> + Send + 'static>,
		>,
		end_witnessing_receiver: oneshot::Receiver<BlockNumber<Self>>,
		mut state: Self::StaticState,
	) -> Result<Self::StaticState, ()>;

	async fn do_witness(
		&mut self,
		data: Self::Data,
		state: &mut Self::StaticState,
	) -> anyhow::Result<()>;
}

pub type WitnesserAndStream<W> =
	(W, Pin<Box<dyn Stream<Item = anyhow::Result<<W as EpochWitnesser>::Data>> + Send + 'static>>);

#[async_trait]
pub trait EpochWitnesserGenerator: Send {
	type Witnesser: EpochWitnesser;

	async fn init(
		&mut self,
		epoch: EpochStart<<Self::Witnesser as EpochWitnesser>::Chain>,
	) -> anyhow::Result<Option<WitnesserAndStream<Self::Witnesser>>>;
}

type WitnesserTask<Witnesser> = ScopedJoinHandle<<Witnesser as EpochWitnesser>::StaticState>;

pub async fn start_epoch_witnesser<Generator>(
	epoch_start_receiver: Arc<
		Mutex<
			async_broadcast::Receiver<EpochStart<<Generator::Witnesser as EpochWitnesser>::Chain>>,
		>,
	>,
	mut witnesser_generator: Generator,
	initial_state: <Generator::Witnesser as EpochWitnesser>::StaticState,
) -> Result<(), ()>
where
	Generator: EpochWitnesserGenerator,
{
	task_scope(|scope| {
		async {
			info!("Starting");

			let mut option_state = Some(initial_state);
			let mut current_task: Option<(
				oneshot::Sender<BlockNumber<Generator::Witnesser>>,
				WitnesserTask<Generator::Witnesser>,
			)> = None;

			let mut epoch_start_receiver =
				epoch_start_receiver.try_lock().expect("should have exclusive ownership");

			loop {
				let epoch_start = epoch_start_receiver.recv().await.expect("Sender closed");

				if let Some((end_witnessing_sender, handle)) = current_task.take() {
					// Send a signal to the previous epoch to stop at the starting block of the new
					// epoch
					let last_block_number_in_epoch = epoch_start
						.block_number
						.checked_sub(&BlockNumber::<Generator::Witnesser>::from(1u32))
						.expect("only the first epoch can start from 0");
					end_witnessing_sender.send(last_block_number_in_epoch).unwrap();

					assert!(
						option_state.replace(handle.await).is_none(),
						"state must have been consumed by generator if we have started a new task"
					);
				}

				if epoch_start.participant &&
					(epoch_start.current ||
						<Generator::Witnesser>::SHOULD_PROCESS_HISTORICAL_EPOCHS)
				{
					info!("Start witnessing from block: {}", epoch_start.block_number);

					let (end_witnessing_sender, end_witnessing_receiver) = oneshot::channel();

					if let Some((witnesser, data_stream)) =
						witnesser_generator.init(epoch_start).await.map_err(|e| {
							error!("Error while initializing epoch witnesser: {:?}", e);
						})? {
						current_task = Some((
							end_witnessing_sender,
							scope.spawn_with_handle(witnesser.run_witnesser(
								data_stream,
								end_witnessing_receiver,
								option_state.take().expect("state must be present"),
							)),
						));
					};
				}
			}
		}
		.boxed()
	})
	.await
}

pub async fn run_witnesser_data_stream<Witnesser>(
	mut witnesser: Witnesser,
	mut data_stream: std::pin::Pin<
		Box<dyn futures::Stream<Item = anyhow::Result<Witnesser::Data>> + Send + 'static>,
	>,
	end_witnessing_receiver: oneshot::Receiver<BlockNumber<Witnesser>>,
	mut state: Witnesser::StaticState,
) -> Result<Witnesser::StaticState, ()>
where
	Witnesser: EpochWitnesser,
{
	let mut end_witnessing_receiver = end_witnessing_receiver.fuse();

	loop {
		select! {
			Ok(_) = &mut end_witnessing_receiver => {
				break;
			},
			Some(data) = data_stream.next() => {
				// This will be an error if the stream times out. When it does, we return
				// an error so that we restart the witnesser

				witnesser.do_witness(data.map_err(|e| {
						error!("Error while getting data for witnesser: {:?}", e);
					})?,
					&mut state).await.map_err(|_| {
					error!("Witnesser failed to process data")
				})?;
			},
		}
	}

	info!("Epoch witnesser finished epoch");

	Ok(state)
}

fn should_end_witnessing<W: EpochWitnesser>(
	last_processed_block: Option<BlockNumber<W>>,
	last_block_in_epoch: Option<BlockNumber<W>>,
) -> bool {
	match (last_processed_block, last_block_in_epoch) {
		(Some(last_processed_block), Some(last_block_in_epoch)) =>
			last_processed_block >= last_block_in_epoch,
		// We continue witnessing if we don't know when the epoch ends
		// or which blocks we have already processed
		_ => false,
	}
}

pub async fn run_witnesser_block_stream<Witnesser>(
	mut witnesser: Witnesser,
	mut block_stream: std::pin::Pin<
		Box<dyn futures::Stream<Item = anyhow::Result<Witnesser::Data>> + Send + 'static>,
	>,
	end_witnessing_receiver: oneshot::Receiver<BlockNumber<Witnesser>>,
	mut state: Witnesser::StaticState,
) -> Result<Witnesser::StaticState, ()>
where
	Witnesser: EpochWitnesser,
	Witnesser::Data: BlockNumberable<BlockNumber = BlockNumber<Witnesser>>,
{
	// If set, this is the last block to process
	let mut last_block_number_for_epoch: Option<BlockNumber<Witnesser>> = None;
	let mut last_processed_block = None;

	let mut end_witnessing_receiver = end_witnessing_receiver.fuse();

	loop {
		select! {
			Ok(last_block_number) = &mut end_witnessing_receiver => {
				last_block_number_for_epoch = Some(last_block_number);

				if should_end_witnessing::<Witnesser>(last_processed_block, last_block_number_for_epoch) {
					break;
				}

			},
			Some(block) = block_stream.next() => {
				// This will be an error if the stream times out. When it does, we return
				// an error so that we restart the witnesser.
				let block = block.map_err(|e| {
					error!("Error while getting block for witnesser: {:?}", e);
				})?;

				let block_number = block.block_number();

				witnesser.do_witness(block, &mut state).await.map_err(|_| {
					error!("Witnesser failed to process block")
				})?;

				last_processed_block = Some(block_number);

				if should_end_witnessing::<Witnesser>(last_processed_block, last_block_number_for_epoch) {
					break;
				}

			},
		}
	}

	info!("Epoch witnesser finished epoch");

	Ok(state)
}

#[cfg(test)]
mod epoch_witnesser_testing {

	use crate::testing::recv_with_timeout;

	use super::*;

	struct TestEpochWitnesser {
		last_processed_block: u64,
		processed_blocks_sender: tokio::sync::mpsc::UnboundedSender<u64>,
	}

	#[async_trait]
	impl EpochWitnesser for TestEpochWitnesser {
		type Chain = cf_chains::Ethereum;

		type Data = u64;

		type StaticState = ();

		const SHOULD_PROCESS_HISTORICAL_EPOCHS: bool = true;

		async fn run_witnesser(
			self,
			data_stream: std::pin::Pin<
				Box<dyn futures::Stream<Item = anyhow::Result<Self::Data>> + Send + 'static>,
			>,
			end_witnessing_receiver: oneshot::Receiver<BlockNumber<Self>>,
			state: Self::StaticState,
		) -> Result<Self::StaticState, ()> {
			run_witnesser_block_stream(self, data_stream, end_witnessing_receiver, state).await
		}

		async fn do_witness(&mut self, block: u64, _: &mut ()) -> anyhow::Result<()> {
			self.last_processed_block = block;
			self.processed_blocks_sender.send(block).unwrap();
			Ok(())
		}
	}

	struct TestEpochWitnesserGenerator {
		processed_blocks_sender: tokio::sync::mpsc::UnboundedSender<u64>,
		block_subscriber: BlockSubscriber,
	}

	impl TestEpochWitnesserGenerator {
		pub fn new() -> (async_channel::Sender<u64>, tokio::sync::mpsc::UnboundedReceiver<u64>, Self)
		{
			let (processed_blocks_sender, processed_blocks_receiver) =
				tokio::sync::mpsc::unbounded_channel();

			let (block_sender, block_receiver) = async_channel::unbounded();

			(
				block_sender,
				processed_blocks_receiver,
				TestEpochWitnesserGenerator {
					processed_blocks_sender,
					block_subscriber: BlockSubscriber { block_receiver },
				},
			)
		}
	}

	#[async_trait]
	impl EpochWitnesserGenerator for TestEpochWitnesserGenerator {
		type Witnesser = TestEpochWitnesser;

		async fn init(
			&mut self,
			epoch_start: EpochStart<cf_chains::Ethereum>,
		) -> anyhow::Result<Option<WitnesserAndStream<TestEpochWitnesser>>> {
			Ok(Some((
				TestEpochWitnesser {
					last_processed_block: epoch_start.block_number,
					processed_blocks_sender: self.processed_blocks_sender.clone(),
				},
				self.block_subscriber.block_stream_from(epoch_start.block_number),
			)))
		}
	}

	struct BlockSubscriber {
		block_receiver: async_channel::Receiver<u64>,
	}

	impl BlockSubscriber {
		fn block_stream_from(
			&mut self,
			block_number: u64,
		) -> Pin<Box<dyn Stream<Item = anyhow::Result<u64>> + Send>> {
			let block_receiver = self.block_receiver.clone();

			block_receiver
				.skip_while(move |block| futures::future::ready(*block < block_number))
				.map(Ok)
				.boxed()
		}
	}

	struct EpochStarter {
		epoch_index: u32,
		epoch_start_sender: async_broadcast::Sender<EpochStart<cf_chains::Ethereum>>,
	}

	impl EpochStarter {
		async fn start(&mut self, block_number: u64, participant: bool) {
			self.epoch_start_sender
				.broadcast(EpochStart {
					epoch_index: self.epoch_index,
					block_number,
					current: true,
					participant,
					data: (),
				})
				.await
				.unwrap();

			self.epoch_index += 1;
		}
	}

	#[tokio::test]
	async fn epoch_witnesser_only_processes_active_epochs() {
		use std::time::Duration;

		let (epoch_start_sender, epoch_start_receiver) = async_broadcast::broadcast(1);

		let (block_sender, mut processed_blocks_receiver, epoch_witnesser_generator) =
			TestEpochWitnesserGenerator::new();
		let mut epoch_starter = EpochStarter { epoch_index: 0, epoch_start_sender };

		tokio::spawn(start_epoch_witnesser(
			Arc::new(Mutex::new(epoch_start_receiver)),
			epoch_witnesser_generator,
			(),
		));

		use crate::testing::expect_recv_with_timeout;

		// Send start of epoch from block 0
		epoch_starter.start(0, true).await;

		// Send block 0, should be witnessed
		block_sender.send(0).await.unwrap();
		assert_eq!(expect_recv_with_timeout(&mut processed_blocks_receiver).await, 0);

		// Send start of epoch from block 2 (not a participant), should still process block 1
		epoch_starter.start(2, false).await;
		// Add a small delay to prevent the witnesser from spuriously processing the next block
		// (if the events are received out of order):
		tokio::time::sleep(Duration::from_millis(10)).await;
		block_sender.send(1).await.unwrap();
		assert_eq!(expect_recv_with_timeout(&mut processed_blocks_receiver).await, 1);

		// Not active in epoch, so should ignore block 2
		block_sender.send(2).await.unwrap();
		assert_eq!(recv_with_timeout(&mut processed_blocks_receiver).await, None);

		// Send start of epoch from block 4 (participant), we should still ignore block 3
		epoch_starter.start(4, true).await;
		block_sender.send(3).await.unwrap();

		// Should process block 4, the first block in the new epoch
		block_sender.send(4).await.unwrap();
		assert_eq!(expect_recv_with_timeout(&mut processed_blocks_receiver).await, 4);

		// Send start of epoch from block 5 (non-participant), should ignore block 5
		epoch_starter.start(5, false).await;
		// Add a small delay to prevent the witnesser from spuriously processing the next block
		// (if the events are received out of order):
		tokio::time::sleep(Duration::from_millis(10)).await;
		block_sender.send(5).await.unwrap();
		assert_eq!(recv_with_timeout(&mut processed_blocks_receiver).await, None);
	}
}
