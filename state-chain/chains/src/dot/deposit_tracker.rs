use super::{PolkadotBalance, PolkadotTrackedData};
use crate::{Chain, DepositChannel, DepositTracker};
use cf_primitives::{chains::Polkadot, ChannelId};
use codec::{Decode, Encode, EncodeLike};
use frame_support::{
	pallet_prelude::RuntimeDebug,
	sp_runtime::traits::{Saturating, Zero},
};
use scale_info::TypeInfo;
use sp_std::{collections::btree_map::BTreeMap, prelude::*};

#[derive(Clone, Default, RuntimeDebug, PartialEq, Eq, Decode, TypeInfo)]
pub struct PolkadotDepositTracker {
	fetched: PolkadotBalance,
	unfetched: BTreeMap<ChannelId, PolkadotBalance>,
}

impl EncodeLike for PolkadotDepositTracker {}

/// A custom encoder that removes any empty unfetched channels before encoding.
impl Encode for PolkadotDepositTracker {
	fn size_hint(&self) -> usize {
		self.fetched.size_hint() + self.unfetched.size_hint()
	}

	fn encode_to<T: codec::Output + ?Sized>(&self, dest: &mut T) {
		self.fetched.encode_to(dest);
		let unfetched = self
			.unfetched
			.iter()
			.filter(|(_, balance)| !balance.is_zero())
			.collect::<Vec<_>>();
		unfetched.encode_to(dest);
	}
}

impl DepositTracker<Polkadot> for PolkadotDepositTracker {
	fn total(&self) -> PolkadotBalance {
		self.unfetched
			.values()
			.copied()
			.sum::<PolkadotBalance>()
			.saturating_add(self.fetched)
	}

	fn register_deposit(
		&mut self,
		amount: PolkadotBalance,
		_deposit_details: &<Polkadot as Chain>::DepositDetails,
		deposit_channel: &<Polkadot as Chain>::DepositChannel,
	) {
		self.unfetched
			.entry(deposit_channel.channel_id())
			.and_modify(|balance| balance.saturating_accrue(amount))
			.or_insert(amount);
	}

	fn withdraw_all(&mut self, _: &PolkadotTrackedData) -> (Vec<ChannelId>, PolkadotBalance) {
		let total = self.total();
		self.fetched = Zero::zero();
		let to_fetch = self.unfetched.keys().copied().collect::<Vec<_>>();
		self.unfetched.clear();
		(to_fetch, total)
	}

	fn withdraw_at_least(
		&mut self,
		amount_to_withdraw: PolkadotBalance,
		_: &PolkadotTrackedData,
	) -> Option<(Vec<ChannelId>, PolkadotBalance)> {
		let mut to_fetch = vec![];

		// First we use any funds that have already been fetched.
		let mut withdrawn = if self.fetched > amount_to_withdraw {
			self.fetched.saturating_reduce(amount_to_withdraw);
			amount_to_withdraw
		} else {
			let withdrawn = self.fetched;
			self.fetched = Zero::zero();
			withdrawn
		};

		let mut withdraw = |channel_id: ChannelId, deposit_balance: &mut u128| -> u128 {
			to_fetch.push(channel_id);
			core::mem::take(deposit_balance)
		};

		// Then we use any unfetched funds.
		for (channel_id, deposit_balance) in self.unfetched.iter_mut() {
			if withdrawn > amount_to_withdraw {
				// One more than we need to reduce address fragmentation.
				withdrawn.saturating_accrue(withdraw(*channel_id, deposit_balance));
				break
			} else {
				withdrawn.saturating_accrue(withdraw(*channel_id, deposit_balance));
			}
		}

		if withdrawn < amount_to_withdraw {
			None
		} else {
			Some((to_fetch, withdrawn))
		}
	}

	/// Polkadot deposit channels can be safely recycled. Above channel Id u16::MAX, fetching is
	/// more expensive, so we don't recycle these.
	fn maybe_recycle_channel(
		&mut self,
		channel: <Polkadot as Chain>::DepositChannel,
	) -> Option<<Polkadot as Chain>::DepositChannel> {
		if channel.channel_id() < u16::MAX.into() {
			Some(channel)
		} else {
			None
		}
	}

	fn on_fetch_completed(&mut self, _channel: &<Polkadot as Chain>::DepositChannel) {}
}
