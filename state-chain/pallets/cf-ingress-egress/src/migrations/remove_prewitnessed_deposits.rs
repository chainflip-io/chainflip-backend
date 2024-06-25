use crate::*;
use frame_support::traits::OnRuntimeUpgrade;

mod old {
	use super::*;
	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct PrewitnessedDeposit<C: Chain> {
		pub asset: C::ChainAsset,
		pub amount: C::ChainAmount,
		pub deposit_address: C::ChainAccount,
		pub block_height: C::ChainBlockNumber,
		pub deposit_details: C::DepositDetails,
	}

	#[frame_support::storage_alias]
	pub type PrewitnessedDeposits<T: Config<I>, I: 'static> = StorageDoubleMap<
		Pallet<T, I>,
		Twox64Concat,
		ChannelId,
		Twox64Concat,
		PrewitnessedDepositId,
		PrewitnessedDeposit<<T as Config<I>>::TargetChain>,
	>;

	#[derive(CloneNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum BoostStatus {
		Boosted { prewitnessed_deposit_id: PrewitnessedDepositId, pools: Vec<BoostPoolTier> },
		NotBoosted,
	}

	#[derive(CloneNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static> {
		pub deposit_channel: DepositChannel<T::TargetChain>,
		pub opened_at: TargetChainBlockNumber<T, I>,
		pub expires_at: TargetChainBlockNumber<T, I>,
		pub action: ChannelAction<T::AccountId>,
		pub boost_fee: BasisPoints,
		// Using the old BoostStatus here
		pub boost_status: BoostStatus,
	}

	#[frame_support::storage_alias]
	pub type DepositChannelLookup<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		TargetChainAccount<T, I>,
		DepositChannelDetails<T, I>,
		OptionQuery,
	>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		let _ = old::PrewitnessedDeposits::<T, I>::clear(u32::MAX, None);

		// convert to new BoostStatus
		DepositChannelLookup::<T, I>::translate_values::<old::DepositChannelDetails<T, I>, _>(
			|old| {
				let boost_status =
					if let old::BoostStatus::Boosted { prewitnessed_deposit_id, pools } =
						old.boost_status
					{
						BoostStatus::Boosted {
							prewitnessed_deposit_id,
							pools,
							// we don't try and backfill these old values
							amount: Zero::zero(),
						}
					} else {
						BoostStatus::NotBoosted
					};

				Some(DepositChannelDetails::<T, I> {
					deposit_channel: old.deposit_channel,
					opened_at: old.opened_at,
					expires_at: old.expires_at,
					action: old.action,
					boost_fee: old.boost_fee,
					boost_status,
				})
			},
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(DepositChannelLookup::<T, I>::iter_values()
			.map(|v| v.boost_status)
			.collect::<Vec<_>>()
			.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let old_values: Vec<old::BoostStatus> =
			Vec::decode(&mut &state[..]).map_err(|_| DispatchError::Other("decode error"))?;

		let new_boost_status = DepositChannelLookup::<T, I>::iter_values()
			.map(|v| v.boost_status)
			.collect::<Vec<_>>();

		for (old, new) in old_values.iter().zip(new_boost_status.iter()) {
			match (old, new) {
				(old::BoostStatus::NotBoosted, BoostStatus::NotBoosted) => {},
				(
					old::BoostStatus::Boosted { prewitnessed_deposit_id, pools },
					BoostStatus::Boosted {
						prewitnessed_deposit_id: new_prewitnessed_deposit_id,
						pools: new_pools,
						amount,
					},
				) => {
					assert_eq!(prewitnessed_deposit_id, new_prewitnessed_deposit_id);
					assert_eq!(pools, new_pools);
					assert_eq!(amount, &Zero::zero());
				},
				_ => panic!("Boost status mismatch"),
			}
		}

		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	use super::*;
	use cf_chains::btc::{deposit_address::DepositAddress, UtxoId};
	use sp_core::H256;

	#[test]
	fn test_migration() {
		use cf_chains::btc::ScriptPubkey;

		use super::*;
		use crate::mock_btc::*;

		new_test_ext().execute_with(|| {
			let address1 = ScriptPubkey::Taproot([0u8; 32]);

			old::PrewitnessedDeposits::<Test, _>::insert(
				1,
				2,
				old::PrewitnessedDeposit {
					asset: cf_chains::assets::btc::Asset::Btc,
					amount: 0,
					deposit_address: address1.clone(),
					block_height: 0,
					deposit_details: UtxoId { tx_id: H256::zero(), vout: 0 },
				},
			);

			let btc_deposit_script: ScriptPubkey = DepositAddress::new([0; 32], 9).script_pubkey();

			let deposit_channel = DepositChannel::<Bitcoin> {
				channel_id: Default::default(),
				address: btc_deposit_script.clone(),
				asset: cf_chains::assets::btc::Asset::Btc,
				state: DepositAddress::new([0; 32], 1),
			};

			old::DepositChannelLookup::<Test, _>::insert(
				btc_deposit_script.clone(),
				old::DepositChannelDetails {
					deposit_channel: deposit_channel.clone(),
					opened_at: 420,
					expires_at: 6969,
					action: ChannelAction::<u64>::LiquidityProvision { lp_account: 22 },
					boost_fee: 8,
					boost_status: old::BoostStatus::Boosted {
						prewitnessed_deposit_id: 2,
						pools: vec![5],
					},
				},
			);

			assert_eq!(DepositChannelLookup::<Test, _>::iter_keys().count(), 1);
			assert_eq!(old::DepositChannelLookup::<Test, _>::iter_keys().count(), 1);

			// Perform runtime migration.
			#[cfg(feature = "try-runtime")]
			let state = super::Migration::<Test, _>::pre_upgrade().unwrap();
			super::Migration::<Test, _>::on_runtime_upgrade();
			#[cfg(feature = "try-runtime")]
			super::Migration::<Test, _>::post_upgrade(state).unwrap();

			// Test that we delete all entries as part of the migration:
			assert_eq!(old::PrewitnessedDeposits::<Test, _>::iter_keys().count(), 0);

			assert_eq!(DepositChannelLookup::<Test, _>::iter_keys().count(), 1);
			assert_eq!(
				DepositChannelLookup::<Test, _>::get(btc_deposit_script),
				Some(DepositChannelDetails {
					deposit_channel: deposit_channel.clone(),
					opened_at: 420,
					expires_at: 6969,
					action: ChannelAction::<u64>::LiquidityProvision { lp_account: 22 },
					boost_fee: 8,
					boost_status: BoostStatus::Boosted {
						prewitnessed_deposit_id: 2,
						pools: vec![5],
						amount: 0,
					},
				})
			);
		});
	}
}
