use crate::{Instance1, Instance2, Instance3, *};
use cf_chains::{
	btc::ScriptPubkey, dot::PolkadotAccountId, Bitcoin, DepositChannel, Ethereum, Polkadot,
};
use frame_support::traits::OnRuntimeUpgrade;
pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

mod old {
	use super::*;

	#[derive(
		CloneNoBound, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen,
	)]
	#[scale_info(skip_type_params(T, I))]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static> {
		pub deposit_channel: DepositChannel<T::TargetChain>,
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

impl<T: Config<Instance1, TargetChain = Ethereum>> OnRuntimeUpgrade for Migration<T, Instance1> {
	fn on_runtime_upgrade() -> Weight {
		DepositChannelLookup::<T, Instance1>::translate(
			|_address: <cf_chains::Ethereum as cf_chains::Chain>::ChainAccount,
			 old_channel: old::DepositChannelDetails<T, Instance1>| {
				Some(DepositChannelDetails::<T, Instance1> {
					deposit_channel: old_channel.deposit_channel,
					opened_at: old_channel.opened_at,
					expires_at: old_channel.expires_at,
					action: old_channel.action,
					boost_fee: 0,
				})
			},
		);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let number_of_channels_in_lookup =
			old::DepositChannelLookup::<T, Instance1>::iter_keys().count() as u32;

		Ok(number_of_channels_in_lookup.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let number_of_channels_in_lookup_pre_migration = <u32>::decode(&mut &state[..]).unwrap();
		ensure!(
			DepositChannelLookup::<T, Instance1>::iter_keys().count() as u32 ==
				number_of_channels_in_lookup_pre_migration,
			"DepositChannelLookup migration failed."
		);
		Ok(())
	}
}

impl<T: Config<Instance2, TargetChain = Polkadot>> OnRuntimeUpgrade for Migration<T, Instance2> {
	fn on_runtime_upgrade() -> Weight {
		DepositChannelLookup::<T, Instance2>::translate(
			|_address: PolkadotAccountId, old_channel: old::DepositChannelDetails<T, Instance2>| {
				Some(DepositChannelDetails::<T, Instance2> {
					deposit_channel: old_channel.deposit_channel,
					opened_at: old_channel.opened_at,
					expires_at: old_channel.expires_at,
					action: old_channel.action,
					boost_fee: 0,
				})
			},
		);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let number_of_channels_in_lookup =
			old::DepositChannelLookup::<T, Instance2>::iter_keys().count() as u32;

		Ok(number_of_channels_in_lookup.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let number_of_channels_in_lookup_pre_migration = <u32>::decode(&mut &state[..]).unwrap();
		ensure!(
			DepositChannelLookup::<T, Instance2>::iter_keys().count() as u32 ==
				number_of_channels_in_lookup_pre_migration,
			"DepositChannelLookup migration failed."
		);
		Ok(())
	}
}

impl<T: Config<Instance3, TargetChain = Bitcoin>> OnRuntimeUpgrade for Migration<T, Instance3> {
	fn on_runtime_upgrade() -> Weight {
		DepositChannelLookup::<T, Instance3>::translate(
			|_address: ScriptPubkey, old_channel: old::DepositChannelDetails<T, Instance3>| {
				Some(DepositChannelDetails::<T, Instance3> {
					deposit_channel: old_channel.deposit_channel,
					opened_at: old_channel.opened_at,
					expires_at: old_channel.expires_at,
					action: old_channel.action,
					boost_fee: 0,
				})
			},
		);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let number_of_channels_in_lookup =
			old::DepositChannelLookup::<T, Instance3>::iter_keys().count() as u32;

		Ok(number_of_channels_in_lookup.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let number_of_channels_in_lookup_pre_migration = <u32>::decode(&mut &state[..]).unwrap();
		ensure!(
			DepositChannelLookup::<T, Instance3>::iter_keys().count() as u32 ==
				number_of_channels_in_lookup_pre_migration,
			"DepositChannelLookup migration failed."
		);
		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	use cf_chains::btc::{
		deposit_address::{DepositAddress, TapscriptPath},
		BitcoinScript, ScriptPubkey,
	};

	use self::mock_btc::new_test_ext;

	use super::*;
	use crate::mock_btc::*;

	#[test]
	fn test_migration() {
		new_test_ext().execute_with(|| {
			let address1 = ScriptPubkey::Taproot([0u8; 32]);
			let address2 = ScriptPubkey::Taproot([1u8; 32]);

			// Insert mock data into old storage
			old::DepositChannelLookup::insert(
				address1.clone(),
				old::DepositChannelDetails::<Test, _> {
					deposit_channel: DepositChannel {
						channel_id: 123,
						address: address2.clone(),
						asset: <Bitcoin as Chain>::ChainAsset::Btc,
						state: DepositAddress {
							pubkey_x: [1u8; 32],
							script_path: Some(TapscriptPath {
                                salt: 123,
								tweaked_pubkey_bytes: [2u8; 33],
								tapleaf_hash: [3u8; 32],
								unlock_script: BitcoinScript::new(Default::default()),
                            }),
						},
					},
					opened_at: Default::default(),
					expires_at: Default::default(),
					action: ChannelAction::LiquidityProvision { lp_account: Default::default() },
				},
			);
			old::DepositChannelLookup::insert(
				address2.clone(),
				old::DepositChannelDetails::<Test, _> {
					deposit_channel: DepositChannel {
						channel_id: 123,
						address: address2.clone(),
						asset: <Bitcoin as Chain>::ChainAsset::Btc,
						state: DepositAddress {
							pubkey_x: [1u8; 32],
							script_path: Some(TapscriptPath {
                                salt: 123,
								tweaked_pubkey_bytes: [2u8; 33],
								tapleaf_hash: [3u8; 32],
								unlock_script: BitcoinScript::new(Default::default()),
                            }),
						},
					},
					opened_at: Default::default(),
					expires_at: Default::default(),
					action: ChannelAction::LiquidityProvision { lp_account: Default::default() },
				},
			);

			// Perform runtime migration.
			// crate::migrations::deposit_channels_with_boost_fee::Migration::<Test, Instance1>::on_runtime_upgrade();
			// Perform runtime migration.
			// crate::migrations::deposit_channels_with_boost_fee::Migration::<Test, Instance2>::on_runtime_upgrade();
			crate::migrations::deposit_channels_with_boost_fee::Migration::<Test, Instance3>::on_runtime_upgrade();

			// Verify data is correctly migrated into new storage.
			let channel = DepositChannelLookup::<Test, Instance3>::get(address1);
			assert!(channel.is_some());
			assert_eq!(channel.unwrap().boost_fee, 0);
			let channel = DepositChannelLookup::<Test, Instance3>::get(address2);
            assert!(channel.is_some());
            assert_eq!(channel.unwrap().boost_fee, 0);
		});
	}
}
