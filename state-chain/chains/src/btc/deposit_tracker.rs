use super::*;
use crate::{DepositChannel, DepositTracker};
use cf_primitives::ChannelId;
use sp_std::collections::btree_map::BTreeMap;

#[derive(Clone, Default, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct BitcoinDepositTracker {
	// TODO: consider splitting this into deposits and change utxos, or to split by controlling key
	// so that we can prioritise spending older utxos.
	utxos: BTreeMap<Utxo, ChannelId>,
	channels: BTreeMap<ChannelId, BitcoinDepositChannel>,
}

// TODO: garbage-collect old channels.
const CHANNEL_INVARIANT: &str = "Channels are only removed if there are no associated utxos.";

impl DepositTracker<Bitcoin> for BitcoinDepositTracker {
	fn total(&self) -> BtcAmount {
		self.utxos.keys().map(|utxo| utxo.amount).sum::<BtcAmount>()
	}

	fn register_deposit(
		&mut self,
		amount: BtcAmount,
		deposit_details: &<Bitcoin as Chain>::DepositDetails,
		deposit_channel: &<Bitcoin as Chain>::DepositChannel,
	) {
		self.utxos
			.insert(Utxo { amount, id: *deposit_details }, deposit_channel.channel_id());
	}

	fn withdraw_all(
		&mut self,
		tracked_data: &BitcoinTrackedData,
	) -> (Vec<BitcoinFetchParams>, BtcAmount) {
		let BitcoinFeeInfo { fee_per_input_utxo, fee_per_output_utxo, min_fee_required_per_tx } =
			tracked_data.btc_fee_info;

		let spendable_utxos: Vec<_> = sp_std::mem::take(&mut self.utxos)
			.into_iter()
			.filter(|(utxo, _channel_id)| utxo.amount > fee_per_input_utxo)
			.collect();

		let total_fee = spendable_utxos.len() as u64 * fee_per_input_utxo +
			fee_per_output_utxo +
			min_fee_required_per_tx;

		spendable_utxos
			.iter()
			.map(|(utxo, _channel_id)| utxo.amount)
			.sum::<u64>()
			.checked_sub(total_fee)
			.map(|change_amount| {
				(
					spendable_utxos
						.into_iter()
						.map(|(utxo, channel_id)| BitcoinFetchParams {
							utxo,
							deposit_address: self
								.channels
								.get(&channel_id)
								.expect(CHANNEL_INVARIANT)
								.clone(),
						})
						.collect(),
					change_amount,
				)
			})
			.unwrap_or_default()
	}

	fn withdraw_at_least(
		&mut self,
		amount: BtcAmount,
		tracked_data: &BitcoinTrackedData,
	) -> Option<(Vec<BitcoinFetchParams>, BtcAmount)> {
		utxo_selection::select_utxos_from_pool(
			&mut self.utxos.keys().cloned().collect(),
			tracked_data.btc_fee_info.fee_per_input_utxo,
			amount,
		)
		.map(|(utxos, total)| {
			(
				utxos
					.into_iter()
					.map(|utxo| {
						let deposit_address = self
							.channels
							.get(
								&self
									.utxos
									.remove(&utxo)
									.expect("Utxo selected from set above, so must exist."),
							)
							.expect(CHANNEL_INVARIANT)
							.clone();
						BitcoinFetchParams { utxo, deposit_address }
					})
					.collect(),
				total,
			)
		})
	}

	fn maybe_recycle_channel(
		&mut self,
		channel: <Bitcoin as Chain>::DepositChannel,
	) -> Option<<Bitcoin as Chain>::DepositChannel> {
		if self.utxos.values().all(|id| *id != channel.channel_id()) {
			self.channels.remove(&channel.channel_id());
		}
		None
	}
}

// fn consolidate(
// 	params: ConsolidationParameters,
// 	tracked_data: &BitcoinTrackedData,) -> todo!() {
// 	let utxos_to_consolidate = utxo_selection_type::select_utxos_for_consolidation(
// 		&mut self.utxos.keys().cloned().collect(),
// 		tracked_data.btc_fee_info.fee_per_input_utxo,
// 		params,
// 	);

// 	if utxos_to_consolidate.is_empty() {
// 		Err(())
// 	} else {
// 		let total_fee = utxos_to_consolidate.len() as u64 * fee_per_input_utxo +
// 			fee_per_output_utxo +
// 			min_fee_required_per_tx;

// 		utxos_to_consolidate
// 			.iter()
// 			.map(|utxo| utxo.amount)
// 			.sum::<u64>()
// 			.checked_sub(total_fee)
// 			.map(|change_amount| (utxos_to_consolidate, change_amount))
// 			.ok_or(())
// 	}
// }
