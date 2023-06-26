use std::{
	collections::{BTreeMap, BTreeSet},
	sync::Arc,
};

use crate::{
	common::Signal, state_chain_observer::client, witness::common::STATE_CHAIN_CONNECTION,
};
use cf_chains::Chain;
use cf_primitives::{AccountId, EpochIndex};
use futures::StreamExt;
use futures_core::{Future, Stream};
use futures_util::stream;
use utilities::task_scope::Scope;

use super::common::{CurrentAndFuture, RuntimeHasInstance};

pub struct Epoch<ActiveState, HistoricState> {
	pub epoch: EpochIndex,
	pub active_state: ActiveState,
	pub historic_signal: Signal<HistoricState>,
	pub expired_signal: Signal<()>,
}

#[derive(Clone)]
enum EpochUpdate<ActiveState, HistoricState> {
	Current(ActiveState),
	Historic(HistoricState),
	Expired,
}

#[derive(Clone)]
pub struct EpochSource<'a, 'env, StateChainClient, ActiveState, HistoricState> {
	scope: &'a Scope<'env, anyhow::Error>,
	state_chain_client: Arc<StateChainClient>,
	initial_block_hash: state_chain_runtime::Hash,
	epochs: BTreeMap<EpochIndex, (ActiveState, Option<HistoricState>)>,
	epoch_update_receiver: async_broadcast::Receiver<(
		EpochIndex,
		state_chain_runtime::Hash,
		EpochUpdate<ActiveState, HistoricState>,
	)>,
}
impl<'a, 'env, StateChainClient: client::storage_api::StorageApi + Send + Sync + 'static>
	EpochSource<'a, 'env, StateChainClient, (), ()>
{
	pub async fn new<StateChainStream: client::StateChainStreamApi>(
		scope: &'a Scope<'env, anyhow::Error>,
		mut state_chain_stream: StateChainStream,
		state_chain_client: Arc<StateChainClient>,
	) -> EpochSource<'a, 'env, StateChainClient, (), ()> {
		let (epoch_update_sender, epoch_update_receiver) = async_broadcast::broadcast(1);

		let initial_block_hash = state_chain_stream.cache().block_hash;

		let mut current_epoch = state_chain_client
			.storage_value::<pallet_cf_validator::CurrentEpoch<state_chain_runtime::Runtime>>(
				initial_block_hash,
			)
			.await
			.expect(STATE_CHAIN_CONNECTION);
		let mut historic_epochs = BTreeSet::from_iter(
			state_chain_client
				.storage_map::<pallet_cf_validator::EpochExpiries<state_chain_runtime::Runtime>>(
					initial_block_hash,
				)
				.await
				.expect(STATE_CHAIN_CONNECTION)
				.into_iter()
				.map(|(_, index)| index),
		);
		assert!(!historic_epochs.contains(&current_epoch));

		let active_epochs = historic_epochs
			.iter()
			.map(|epoch| (*epoch, ((), Some(()))))
			.chain(std::iter::once((current_epoch, ((), None))))
			.collect();

		scope.spawn({
			let state_chain_client = state_chain_client.clone();
			async move {
				utilities::loop_select! {
					if epoch_update_sender.is_closed() => let _ = futures::future::ready(()) => {
						break Ok(())
					},
					if let Some((block_hash, _block_header)) = state_chain_stream.next() => {
						let old_current_epoch = std::mem::replace(&mut current_epoch, state_chain_client
							.storage_value::<pallet_cf_validator::CurrentEpoch<
								state_chain_runtime::Runtime,
							>>(block_hash)
							.await
							.expect(STATE_CHAIN_CONNECTION));
						if old_current_epoch != current_epoch {
							let _result = epoch_update_sender.broadcast((old_current_epoch, block_hash, EpochUpdate::Historic(())));
							let _result = epoch_update_sender.broadcast((current_epoch, block_hash, EpochUpdate::Current(())));
							historic_epochs.insert(old_current_epoch);
						}

						let old_historic_epochs = std::mem::replace(&mut historic_epochs, BTreeSet::from_iter(
							state_chain_client.storage_map::<pallet_cf_validator::EpochExpiries<
								state_chain_runtime::Runtime,
							>>(block_hash).await.expect(STATE_CHAIN_CONNECTION).into_iter().map(|(_, index)| index)
						));
						assert!(!historic_epochs.contains(&current_epoch));
						assert!(old_historic_epochs.is_superset(&historic_epochs));
						for expired_epoch in old_historic_epochs.difference(&historic_epochs) {
							let _result = epoch_update_sender.broadcast((*expired_epoch, block_hash, EpochUpdate::Expired));
						}
					} else break Ok(()),
				}
			}
		});

		Self {
			scope,
			state_chain_client,
			initial_block_hash,
			epochs: active_epochs,
			epoch_update_receiver,
		}
	}
}

impl<
		'a,
		'env,
		StateChainClient: client::storage_api::StorageApi + Send + Sync + 'static,
		ActiveState: Clone + Send + Sync + 'static,
		HistoricState: Clone + Send + Sync + 'static,
	> EpochSource<'a, 'env, StateChainClient, ActiveState, HistoricState>
{
	pub async fn into_stream(
		self,
	) -> CurrentAndFuture<
		impl Iterator<Item = Epoch<ActiveState, HistoricState>> + Send + 'static,
		impl Stream<Item = Epoch<ActiveState, HistoricState>> + Send + 'static,
	> {
		let mut historic_signallers = BTreeMap::new();
		let mut expired_signallers = BTreeMap::new();

		let current = self
			.epochs
			.into_iter()
			.map(|(epoch, (active_state, option_historic_state))| {
				let (expired_signaller, expired_signal) = Signal::new();

				expired_signallers.insert(epoch, expired_signaller);

				Epoch {
					epoch,
					active_state,
					historic_signal: match option_historic_state {
						Some(historic_state) => Signal::signalled(historic_state),
						None => {
							let (historic_signaller, historic_signal) = Signal::new();
							historic_signallers.insert(epoch, historic_signaller);
							historic_signal
						},
					},
					expired_signal,
				}
			})
			.collect::<Vec<_>>()
			.into_iter();

		CurrentAndFuture {
			current,
			future: stream::unfold(
				(self.epoch_update_receiver, historic_signallers, expired_signallers),
				|(mut epoch_update_receiver, mut historic_signallers, mut expired_signallers)| async move {
					while let Some((epoch, _block_hash, update)) =
						epoch_update_receiver.next().await
					{
						match update {
							EpochUpdate::Current(active_state) => {
								let (historic_signaller, historic_signal) = Signal::new();
								let (expired_signaller, expired_signal) = Signal::new();

								historic_signallers.insert(epoch, historic_signaller);
								expired_signallers.insert(epoch, expired_signaller);

								return Some((
									Epoch { epoch, active_state, historic_signal, expired_signal },
									(
										epoch_update_receiver,
										historic_signallers,
										expired_signallers,
									),
								))
							},
							EpochUpdate::Historic(historic_state) => {
								historic_signallers.remove(&epoch).unwrap().signal(historic_state);
							},
							EpochUpdate::Expired => {
								expired_signallers.remove(&epoch).unwrap().signal(());
							},
						}
					}

					None
				},
			),
		}
	}

	pub async fn participating<Instance: 'static>(
		self,
		account_id: AccountId,
	) -> EpochSource<'a, 'env, StateChainClient, ActiveState, HistoricState>
	where
		state_chain_runtime::Runtime: RuntimeHasInstance<Instance>,
	{
		self.map(
			move |state_chain_client, epoch, block_hash, active_state| {
				let account_id = account_id.clone();
				async move {
					if state_chain_client
						.storage_map_entry::<pallet_cf_validator::HistoricalActiveEpochs<state_chain_runtime::Runtime>>(
							block_hash,
							&account_id,
						)
						.await
						.expect(STATE_CHAIN_CONNECTION)
						.iter()
						.any(|participating_epoch| *participating_epoch == epoch)
					{
						Some(active_state)
					} else {
						None
					}
				}
			},
			|_state_chain_client, _epoch, _block_hash, historic_state| async move { historic_state },
		)
		.await
	}

	async fn map<
		GetActiveState,
		CSFut,
		MappedActiveState,
		GetHistoricState,
		HSFut,
		MappedHistoricState,
	>(
		self,
		get_active_state: GetActiveState,
		get_historic_state: GetHistoricState,
	) -> EpochSource<'a, 'env, StateChainClient, MappedActiveState, MappedHistoricState>
	where
		GetActiveState: Fn(Arc<StateChainClient>, EpochIndex, state_chain_runtime::Hash, ActiveState) -> CSFut
			+ Send
			+ Sync
			+ 'static,
		CSFut: Future<Output = Option<MappedActiveState>> + Send + 'static,
		MappedActiveState: Clone + Send + Sync + 'static,
		GetHistoricState: Fn(Arc<StateChainClient>, EpochIndex, state_chain_runtime::Hash, HistoricState) -> HSFut
			+ Send
			+ Sync
			+ 'static,
		HSFut: Future<Output = MappedHistoricState> + Send + 'static,
		MappedHistoricState: Clone + Send + Sync + 'static,
	{
		let EpochSource {
			scope,
			state_chain_client,
			initial_block_hash,
			epochs: unmapped_epochs,
			epoch_update_receiver: mut unmapped_epoch_update_receiver,
		} = self;

		let (epoch_update_sender, epoch_update_receiver) = async_broadcast::broadcast(1);

		let epochs: BTreeMap<_, _> = futures::stream::iter(unmapped_epochs)
			.filter_map(|(epoch, (active_state, option_historic_state))| {
				let get_active_state = &get_active_state;
				let get_historic_state = &get_historic_state;
				let state_chain_client = &state_chain_client;
				async move {
					if let Some(mapped_active_state) = get_active_state(
						state_chain_client.clone(),
						epoch,
						initial_block_hash,
						active_state,
					)
					.await
					{
						Some((
							epoch,
							(
								mapped_active_state,
								match option_historic_state {
									Some(historic_state) => Some(
										get_historic_state(
											state_chain_client.clone(),
											epoch,
											initial_block_hash,
											historic_state,
										)
										.await,
									),
									None => None,
								},
							),
						))
					} else {
						None
					}
				}
			})
			.collect()
			.await;

		self.scope.spawn({
			let state_chain_client = state_chain_client.clone();
			let mut epochs = epochs.keys().cloned().collect::<BTreeSet<_>>();
			async move {
				utilities::loop_select! {
					if epoch_update_sender.is_closed() => let _ = futures::future::ready(()) => {
						break Ok(())
					},
					if let Some((epoch, block_hash, update)) = unmapped_epoch_update_receiver.next() => {
						match update {
							EpochUpdate::Current(active_state) => {
								if let Some(mapped_active_state) = get_active_state(state_chain_client.clone(), epoch, block_hash, active_state).await {
									epochs.insert(epoch);
									let _result = epoch_update_sender.broadcast((epoch, block_hash, EpochUpdate::Current(mapped_active_state)));
								}
							},
							EpochUpdate::Historic(historic_state) => {
								if epochs.contains(&epoch) {
									let _result = epoch_update_sender.broadcast((
										epoch,
										block_hash,
										EpochUpdate::Historic(get_historic_state(state_chain_client.clone(), epoch, block_hash, historic_state).await),
									));
								}
							},
							EpochUpdate::Expired => {
								epochs.remove(&epoch);
							},
						}
					} else break Ok(()),
				}
			}
		});

		EpochSource { scope, state_chain_client, initial_block_hash, epochs, epoch_update_receiver }
	}
}

pub type Vault<Instance> = Epoch<
	pallet_cf_vaults::Vault<<state_chain_runtime::Runtime as pallet_cf_vaults::Config<Instance>>::Chain>,
	<<state_chain_runtime::Runtime as pallet_cf_vaults::Config<Instance>>::Chain as Chain>::ChainBlockNumber,
>;

pub type VaultSource<'a, 'env, StateChainClient, Instance> = EpochSource<
	'a,
	'env,
	StateChainClient,
	pallet_cf_vaults::Vault<<state_chain_runtime::Runtime as pallet_cf_vaults::Config<Instance>>::Chain>,
	<<state_chain_runtime::Runtime as pallet_cf_vaults::Config<Instance>>::Chain as Chain>::ChainBlockNumber,
>;

impl<'a, 'env, StateChainClient: client::storage_api::StorageApi + Send + Sync + 'static>
	EpochSource<'a, 'env, StateChainClient, (), ()>
{
	pub async fn vaults<Instance: 'static>(
		self,
	) -> VaultSource<'a, 'env, StateChainClient, Instance>
	where
		state_chain_runtime::Runtime: RuntimeHasInstance<Instance>,
	{
		self.map(
			|state_chain_client, epoch, block_hash, ()| async move {
				state_chain_client
					.storage_map_entry::<pallet_cf_vaults::Vaults<state_chain_runtime::Runtime, Instance>>(
						block_hash, &epoch,
					)
					.await
					.expect(STATE_CHAIN_CONNECTION)
			},
			|state_chain_client, epoch, block_hash, ()| async move {
				state_chain_client
					.storage_map_entry::<pallet_cf_vaults::Vaults<state_chain_runtime::Runtime, Instance>>(
						block_hash,
						&(epoch + 1),
					)
					.await
					.expect(STATE_CHAIN_CONNECTION)
					.expect("We know the epoch ended, so the next vault must exist.")
					.active_from_block
			},
		)
		.await
	}
}
