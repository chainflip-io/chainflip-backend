use cf_primitives::BlockWitnesserEvent;
use cf_utilities::{define_empty_struct, hook_impls};
use codec::{Decode, Encode};
use pallet_cf_broadcast::TransactionConfirmation;
use pallet_cf_ingress_egress::{DepositWitness, TargetChainBlockNumber, VaultDepositWitness};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

trait Config<I: 'static> = pallet_cf_ingress_egress::Config<I>
	+ pallet_cf_vaults::Config<I>
	+ pallet_cf_broadcast::Config<I>;

define_empty_struct! {
	pub struct PalletHooks<T: Config<I>, I: 'static>;
}

hook_impls! {
	for PalletHooks<T, I> where (T: Config<I>, I: 'static):

	// --- deposit channel witnessing dispatch ---
	fn(&mut self, (event, block_height): (BlockWitnesserEvent<DepositWitness<T::TargetChain>>, TargetChainBlockNumber<T, I>)) -> () {
		match event {
			BlockWitnesserEvent::PreWitness(deposit_witness) => {
				let _ = pallet_cf_ingress_egress::Pallet::<T, I>::process_channel_deposit_prewitness(
					deposit_witness,
					block_height,
				);
			},
			BlockWitnesserEvent::Witness(deposit_witness) => {
				pallet_cf_ingress_egress::Pallet::<T, I>::process_channel_deposit_full_witness(deposit_witness, block_height);
			},
		}
	}

	// --- vault swap witnessing dispatch ---
	fn(&mut self, (event, block_height): (BlockWitnesserEvent<VaultDepositWitness<T, I>>, TargetChainBlockNumber<T, I>)) -> () {
		match event {
			BlockWitnesserEvent::PreWitness(deposit) => {
				pallet_cf_ingress_egress::Pallet::<T, I>::process_vault_swap_request_prewitness(
					block_height,
					deposit.clone(),
				);
			},
			BlockWitnesserEvent::Witness(deposit) => {
				pallet_cf_ingress_egress::Pallet::<T, I>::process_vault_swap_request_full_witness_inner(
					block_height,
					deposit.clone(),
				);
			},
		}
	}

	// --- egress witnessing dispatch ---
	fn(&mut self, (event, block_height): (BlockWitnesserEvent<TransactionConfirmation<T, I>>, TargetChainBlockNumber<T, I>)) -> ()
	where (
		<T as frame_system::Config>::RuntimeOrigin: From<pallet_cf_witnesser::RawOrigin>,
	)
	{
		match event {
			BlockWitnesserEvent::PreWitness(_) => { /* We don't care about pre-witnessing an egress */
			},
			BlockWitnesserEvent::Witness(egress) => {
				if let Err(err) = pallet_cf_broadcast::Pallet::<T, I>::egress_success(
					pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into(),
					egress.clone(),
					block_height,
				) {
					log::error!(
						"Failed to execute Bitcoin egress success: TxOutId: {:?}, Error: {:?}",
						egress.tx_out_id,
						err
					)
				}
			},
		}
	}
}
