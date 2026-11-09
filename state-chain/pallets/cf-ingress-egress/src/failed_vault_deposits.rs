// Copyright 2026 Chainflip Labs GmbH
// SPDX-License-Identifier: Apache-2.0

use super::*;
use frame_support::Hashable;

/// Identifies the complete witnessed deposit, including distinct deposits in one transaction.
pub type FailedVaultDepositId = [u8; 32];

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub enum FailedVaultDepositCause {
	UnknownBroker,
	RecoveryFailed(DispatchError),
	Unrefundable,
	/// Reported for rejection. Failed deposits have no trusted refund address, so the deposit
	/// can't be returned automatically.
	TransactionRejected,
}

#[derive(
	CloneNoBound,
	DebugNoBound,
	PartialEqNoBound,
	EqNoBound,
	Encode,
	Decode,
	DecodeWithMemTracking,
	GenericTypeInfo,
)]
#[expand_name_with(<T::TargetChain as PalletInstanceAlias>::TYPE_INFO_SUFFIX)]
pub struct FailedVaultDepositInfo<T: Config<I>, I: 'static> {
	pub facts: VaultDepositFacts<T, I>,
	pub block_height: TargetChainBlockNumber<T, I>,
	#[skip_name_expansion]
	pub recorded_at: BlockNumberFor<T>,
	#[skip_name_expansion]
	pub broker_id: Option<T::AccountId>,
	pub ingress_fee: TargetChainAmount<T, I>,
	pub remaining_amount: TargetChainAmount<T, I>,
}

#[derive(
	CloneNoBound,
	DebugNoBound,
	PartialEqNoBound,
	EqNoBound,
	Encode,
	Decode,
	DecodeWithMemTracking,
	GenericTypeInfo,
)]
#[expand_name_with(<T::TargetChain as PalletInstanceAlias>::TYPE_INFO_SUFFIX)]
pub struct FailedVaultDeposit<T: Config<I>, I: 'static> {
	pub deposit: FailedVaultDepositInfo<T, I>,
	#[skip_name_expansion]
	pub cause: FailedVaultDepositCause,
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	pub fn failed_vault_deposit_id(witness: &VaultDepositWitness<T, I>) -> FailedVaultDepositId {
		witness.blake2_256()
	}

	/// Failed and unrefundable witnesses cannot pass prewitness validation for boosting.
	pub(crate) fn process_failed_vault_deposit(
		block_height: TargetChainBlockNumber<T, I>,
		witness: &VaultDepositWitness<T, I>,
	) -> Result<(), DepositFailedReason> {
		let facts = witness.deposit();
		ensure!(
			facts.deposit_amount >= MinimumDeposit::<T, I>::get(facts.input_asset),
			DepositFailedReason::BelowMinimumDeposit
		);
		let rejected = Self::is_marked_for_rejection(
			&facts.deposit_details,
			witness.broker_id(),
			facts.deposit_address.as_ref(),
		);

		T::DepositHandler::on_deposit_made(facts.deposit_details.clone());
		let AmountAndFeesWithheld { amount_after_fees, fees_withheld } =
			Self::withhold_ingress_or_egress_fee(
				IngressOrEgress::IngressVaultSwap,
				facts.input_asset,
				facts.deposit_amount,
			);
		ensure!(!amount_after_fees.is_zero(), DepositFailedReason::NotEnoughToPayFees);
		let deposit = FailedVaultDepositInfo {
			facts: facts.clone(),
			block_height,
			recorded_at: frame_system::Pallet::<T>::block_number(),
			broker_id: witness.broker_id().cloned(),
			ingress_fee: fees_withheld,
			remaining_amount: amount_after_fees,
		};
		let action = if rejected {
			Self::queue_failed_vault_deposit(
				witness,
				deposit,
				FailedVaultDepositCause::TransactionRejected,
			)
		} else {
			Self::recover_or_queue_failed_vault_deposit(witness, deposit)
		};
		Self::deposit_event(Event::DepositFinalised {
			deposit_address: facts.deposit_address.clone(),
			asset: facts.input_asset,
			amount: facts.deposit_amount,
			block_height,
			deposit_details: facts.deposit_details.clone(),
			ingress_fee: fees_withheld,
			max_boost_fee_bps: witness.boost_fee(),
			action,
			channel_id: facts.channel_id,
			origin_type: DepositOriginType::Vault,
		});
		Ok(())
	}

	fn recover_or_queue_failed_vault_deposit(
		witness: &VaultDepositWitness<T, I>,
		deposit: FailedVaultDepositInfo<T, I>,
	) -> DepositAction<T, I> {
		let cause = if let Some(broker_id) = deposit.broker_id.clone() {
			match Self::route_failed_deposit(
				&broker_id,
				deposit.facts.input_asset.into(),
				deposit.remaining_amount.into(),
			) {
				Ok((recovery_account, outcome)) => {
					Self::deposit_event(Event::FailedDepositRecovered {
						queue_id: None,
						deposit,
						broker_id: broker_id.clone(),
						recovery_account: recovery_account.clone(),
						outcome: outcome.clone(),
					});
					return DepositAction::BrokerRecovery { broker_id, recovery_account, outcome }
				},
				Err(error) => FailedVaultDepositCause::RecoveryFailed(error.stripped()),
			}
		} else {
			match witness {
				VaultDepositWitness::Swap { .. } => FailedVaultDepositCause::Unrefundable,
				VaultDepositWitness::Failed { .. } => FailedVaultDepositCause::UnknownBroker,
			}
		};
		Self::queue_failed_vault_deposit(witness, deposit, cause)
	}

	fn queue_failed_vault_deposit(
		witness: &VaultDepositWitness<T, I>,
		deposit: FailedVaultDepositInfo<T, I>,
		cause: FailedVaultDepositCause,
	) -> DepositAction<T, I> {
		let queue_id = Self::failed_vault_deposit_id(witness);
		let entry = FailedVaultDeposit { deposit, cause };
		// Elections deduplicate witnesses before deposit accounting.
		FailedVaultDeposits::<T, I>::insert(queue_id, &entry);
		Self::deposit_event(Event::FailedDepositQueued { queue_id, deposit: entry });
		DepositAction::Queued { queue_id }
	}
}
