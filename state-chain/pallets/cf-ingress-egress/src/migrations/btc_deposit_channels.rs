use crate::{Instance1, Instance2, Instance3, *};
use cf_chains::{
	btc::deposit_address::{DepositAddress, TapscriptPath},
	Bitcoin,
};
use frame_support::traits::OnRuntimeUpgrade;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

mod old {
	use super::*;
	use cf_chains::btc::{BitcoinScript, Hash, ScriptPubkey};

	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct DepositAddress {
		pub pubkey_x: [u8; 32],
		pub salt: u32,
		pub tweaked_pubkey_bytes: [u8; 33],
		pub tapleaf_hash: Hash,
		pub unlock_script: BitcoinScript,
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
	pub struct DepositChannel {
		pub channel_id: ChannelId,
		pub address: ScriptPubkey,
		pub asset: Asset,
		pub state: old::DepositAddress,
	}

	#[derive(
		CloneNoBound, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen,
	)]
	#[scale_info(skip_type_params(T, I))]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static> {
		pub deposit_channel: old::DepositChannel,
		pub opened_at: TargetChainBlockNumber<T, I>,
		pub expires_at: TargetChainBlockNumber<T, I>,
		pub action: ChannelAction<T::AccountId>,
	}

	#[frame_support::storage_alias]
	pub type DepositChannelLookup<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		TargetChainAccount<T, I>,
		old::DepositChannelDetails<T, I>,
		OptionQuery,
	>;
}

impl<T: Config<Instance1>> OnRuntimeUpgrade for Migration<T, Instance1> {
	fn on_runtime_upgrade() -> Weight {
		Weight::zero()
	}
}

impl<T: Config<Instance2>> OnRuntimeUpgrade for Migration<T, Instance2> {
	fn on_runtime_upgrade() -> Weight {
		Weight::zero()
	}
}

impl<T: Config<Instance3, TargetChain = Bitcoin>> OnRuntimeUpgrade for Migration<T, Instance3> {
	fn on_runtime_upgrade() -> Weight {
		let old_channels = old::DepositChannelLookup::<T, Instance3>::drain().collect::<Vec<_>>();
		for (address, old_channel) in old_channels {
			let new_channel = DepositChannelDetails::<T, Instance3> {
				deposit_channel: DepositChannel {
					channel_id: old_channel.deposit_channel.channel_id,
					address: address.clone(),
					asset: old_channel.deposit_channel.asset.try_into().unwrap(),
					state: DepositAddress {
						pubkey_x: old_channel.deposit_channel.state.pubkey_x,
						script_path: Some(TapscriptPath {
							salt: old_channel.deposit_channel.state.salt,
							tweaked_pubkey_bytes: old_channel
								.deposit_channel
								.state
								.tweaked_pubkey_bytes,
							tapleaf_hash: old_channel.deposit_channel.state.tapleaf_hash,
							unlock_script: old_channel.deposit_channel.state.unlock_script,
						}),
					},
				},
				opened_at: old_channel.opened_at,
				expires_at: old_channel.expires_at,
				action: old_channel.action,
			};
			DepositChannelLookup::<T, Instance3>::insert(address, new_channel);
		}

		Weight::zero()
	}
}

#[cfg(test)]
mod migration_tests {
	use cf_chains::btc::{BitcoinScript, ScriptPubkey};

	use self::mock_btc::new_test_ext;

	use super::*;
	use crate::mock_btc::*;

	#[test]
	fn test_migration() {
		new_test_ext().execute_with(|| {
			let address = ScriptPubkey::Taproot([0u8; 32]);
			// Insert mock data into old storage
			old::DepositChannelLookup::insert(
				address.clone(),
				old::DepositChannelDetails::<Test, _> {
					deposit_channel: old::DepositChannel {
						channel_id: 123,
						address: address.clone(),
						asset: Asset::Btc,
						state: old::DepositAddress {
							pubkey_x: [1u8; 32],
							salt: 123,
							tweaked_pubkey_bytes: [2u8; 33],
							tapleaf_hash: [3u8; 32],
							unlock_script: BitcoinScript::new(Default::default()),
						},
					},
					opened_at: Default::default(),
					expires_at: Default::default(),
					action: ChannelAction::LiquidityProvision { lp_account: Default::default() },
				},
			);

			// Perform runtime migration.
			crate::migrations::btc_deposit_channels::Migration::<Test, Instance3>::on_runtime_upgrade();

			// Verify data is correctly migrated into new storage.
			//assert!(DepositChannelLookup::<Test>::get(address).is_some());
			assert!(DepositChannelLookup::<Test, Instance3>::get(address).is_some());
		});
	}
}
