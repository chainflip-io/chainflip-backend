use core::mem::size_of;

use super::{
	deposit_address::EthereumDepositChannel, DeploymentStatus, EthereumTrackedData, EvmFetchId,
};
use crate::{Chain, DepositChannel, DepositTracker};
use cf_primitives::{
	chains::{assets::eth::Asset, Ethereum},
	ChannelId, EthAmount,
};
use codec::{Decode, Encode, EncodeLike};
use fnv::FnvBuildHasher;
use frame_support::{
	pallet_prelude::RuntimeDebug,
	sp_runtime::traits::{Saturating, Zero},
};
use indexmap::IndexMap;
use scale_info::TypeInfo;
use sp_std::prelude::*;

/// An IndexMap that uses FNV hashing. FNV hashing is significantly faster than the default for
/// small inputs, such as in this case where the input is just the channel ID.
type FnvIndexMap<K, V> = IndexMap<K, V, FnvBuildHasher>;

#[derive(Clone, Default, RuntimeDebug, PartialEq, Eq)]
pub struct EthereumDepositTracker {
	fetched: EthAmount,
	unfetched: FnvIndexMap<ChannelId, (EthereumDepositChannel, EthAmount)>,
}

#[derive(Encode, Decode, TypeInfo)]
struct EthereumDepositTrackerStorable {
	fetched: EthAmount,
	unfetched: Vec<(EthereumDepositChannel, EthAmount)>,
}

impl From<EthereumDepositTrackerStorable> for EthereumDepositTracker {
	fn from(storable: EthereumDepositTrackerStorable) -> Self {
		let EthereumDepositTrackerStorable { fetched, unfetched } = storable;
		Self {
			fetched,
			unfetched: FnvIndexMap::from_iter(
				unfetched
					.into_iter()
					.map(|(channel, deposit)| (channel.channel_id(), (channel, deposit))),
			),
		}
	}
}

impl From<EthereumDepositTracker> for EthereumDepositTrackerStorable {
	fn from(tracker: EthereumDepositTracker) -> Self {
		let EthereumDepositTracker { fetched, unfetched } = tracker;
		let mut unfetched = unfetched.into_iter().map(|(_k, v)| v).collect::<Vec<_>>();
		unfetched.sort_unstable_by_key(|(_channel, amount)| Reverse(*amount));
		Self { fetched, unfetched }
	}
}

impl EncodeLike<EthereumDepositTrackerStorable> for EthereumDepositTracker {}

impl Encode for EthereumDepositTracker {
	fn size_hint(&self) -> usize {
		self.fetched.size_hint() +
			self.unfetched.len() *
				(size_of::<ChannelId>() +
					size_of::<EthereumDepositChannel>() +
					size_of::<EthAmount>())
	}

	fn encode_to<T: codec::Output + ?Sized>(&self, dest: &mut T) {
		let storable = EthereumDepositTrackerStorable::from(self.clone());
		storable.encode_to(dest);
	}
}

impl EncodeLike for EthereumDepositTracker {}

impl Decode for EthereumDepositTracker {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let storable = EthereumDepositTrackerStorable::decode(input)?;
		Ok(storable.into())
	}
}

impl TypeInfo for EthereumDepositTracker {
	type Identity = Self;

	fn type_info() -> scale_info::Type {
		EthereumDepositTrackerStorable::type_info()
	}
}

impl DepositTracker<Ethereum> for EthereumDepositTracker {
	fn total(&self) -> EthAmount {
		self.unfetched
			.values()
			.map(|(_, amount)| amount)
			.copied()
			.sum::<EthAmount>()
			.saturating_add(self.fetched)
	}

	fn register_deposit(
		&mut self,
		amount: EthAmount,
		_deposit_details: &<Ethereum as Chain>::DepositDetails,
		deposit_channel: &<Ethereum as Chain>::DepositChannel,
	) {
		if matches!(
			deposit_channel,
			EthereumDepositChannel {
				asset: Asset::Eth,
				deployment_status: DeploymentStatus::Pending | DeploymentStatus::Deployed,
				..
			}
		) {
			self.fetched.saturating_accrue(amount);
		} else {
			self.unfetched
				.insert(deposit_channel.channel_id(), (deposit_channel.clone(), amount));
		}
	}

	fn withdraw_all(&mut self, _: &EthereumTrackedData) -> (Vec<EvmFetchId>, EthAmount) {
		let total = self.total();
		self.fetched = Zero::zero();
		let to_fetch = self
			.unfetched
			.drain(..)
			.map(|(_id, (channel, _deposit_amount))| channel.fetch_params(()))
			.collect::<Vec<_>>();
		(to_fetch, total)
	}

	fn withdraw_at_least(
		&mut self,
		amount_to_withdraw: EthAmount,
		_: &EthereumTrackedData,
	) -> Option<(Vec<EvmFetchId>, EthAmount)> {
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

		// Then we use any funds that are fetchable.
		let fetchable_deposits =
			self.unfetched.iter().filter_map(|(_id, (channel, deposit_amount))| {
				if matches!(channel.deployment_status, DeploymentStatus::Pending) {
					None
				} else {
					Some((channel, deposit_amount))
				}
			});

		fn process_fetch(
			withdrawn: &mut u128,
			deposit_amount: &u128,
			to_fetch: &mut Vec<EvmFetchId>,
			fetched: &mut Vec<EthereumDepositChannel>,
			mut channel: EthereumDepositChannel,
		) {
			withdrawn.saturating_accrue(*deposit_amount);
			to_fetch.push(channel.fetch_params(()));
			if matches!(channel.deployment_status, DeploymentStatus::Undeployed) {
				channel.deployment_status = DeploymentStatus::Pending;
			};
			fetched.push(channel);
		}

		let mut fetched: Vec<_> = Default::default();
		let mut one_more = true;
		for (channel, deposit_amount) in fetchable_deposits {
			process_fetch(
				&mut withdrawn,
				deposit_amount,
				&mut to_fetch,
				&mut fetched,
				channel.clone(),
			);
			if withdrawn >= amount_to_withdraw {
				if one_more {
					process_fetch(
						&mut withdrawn,
						deposit_amount,
						&mut to_fetch,
						&mut fetched,
						channel.clone(),
					);
					one_more = false;
				} else {
					break
				}
			}
		}

		for channel in fetched.into_iter() {
			self.unfetched.insert(channel.channel_id(), (channel, Zero::zero()));
		}

		Some((to_fetch, withdrawn))
	}

	/// A completed fetch should be in either the pending or deployed state. Confirmation of a fetch
	/// implies that the address is now deployed.
	fn on_fetch_completed(&mut self, channel: &<Ethereum as Chain>::DepositChannel) {
		if let Some((ref mut channel, _)) = self.unfetched.get_mut(&channel.channel_id) {
			if channel.deployment_status == DeploymentStatus::Undeployed {
				#[cfg(debug_assertions)]
				{
					panic!("Cannot finalize fetch to an undeployed address")
				}
				#[cfg(not(debug_assertions))]
				{
					log::error!("Cannot finalize fetch to an undeployed address");
				}
			}
			channel.deployment_status = DeploymentStatus::Deployed;
		}
	}

	/// Undeployed Ethereum channels should not be recycled unless they have non-zero deposit
	/// balance.
	///
	/// Other address types can always be recycled.
	fn maybe_recycle_channel(
		&mut self,
		channel: <Ethereum as Chain>::DepositChannel,
	) -> Option<<Ethereum as Chain>::DepositChannel> {
		if let Some(index) = self.unfetched.get_full(&channel.channel_id).and_then(
			|(index, _id, (_channel, deposit_amount))| {
				if channel.deployment_status == DeploymentStatus::Undeployed && *deposit_amount == 0
				{
					Some(index)
				} else {
					None
				}
			},
		) {
			self.unfetched.shift_remove_index(index);
			None
		} else {
			Some(channel)
		}
	}
}
