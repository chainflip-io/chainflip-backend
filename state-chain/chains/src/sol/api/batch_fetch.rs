use core::marker::PhantomData;

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::vec::Vec;

use crate::{
	sol::{
		api::SolanaEnvironment,
		sol_tx_building_blocks::instruction_builder::{
			SolanaInstructionBuilder, SolanaInstructionBuilderError,
		},
		SolMessage, SolTransaction, SolanaCrypto,
	},
	ApiCall, ChainCrypto, DepositChannel, Solana,
};

use frame_support::{CloneNoBound, EqNoBound, PartialEqNoBound, RuntimeDebugNoBound};

/// Represents a Solana Transaction that fetches from Deposit channels, and transfer
/// funds to some target accounts.
#[derive(
	Encode, Decode, TypeInfo, CloneNoBound, RuntimeDebugNoBound, PartialEqNoBound, EqNoBound,
)]
#[scale_info(skip_type_params(Environment))]
pub struct BatchFetches<Environment: 'static> {
	transaction: SolTransaction,
	_phantom: PhantomData<Environment>,
}

impl<Environment: SolanaEnvironment> BatchFetches<Environment> {
	pub fn new_unsigned(
		deposit_channels: Vec<DepositChannel<Solana>>,
	) -> Result<Self, SolanaInstructionBuilderError> {
		// Lookup the current Aggkey
		let agg_key = Environment::lookup_account(super::SolanaEnvAccountLookupKey::AggKey)
			.ok_or(SolanaInstructionBuilderError::CannotLookupAggKey)?;

		// Build the instruction_set
		let instruction_set = SolanaInstructionBuilder::<Environment>::default()
			.fetch_from(deposit_channels)?
			.finalize()?;
		let transaction =
			SolTransaction::new_unsigned(SolMessage::new(&instruction_set, Some(&agg_key.into())));

		Ok(Self { transaction, _phantom: Default::default() })
	}
}

impl<Environment> ApiCall<SolanaCrypto> for BatchFetches<Environment> {
	fn threshold_signature_payload(&self) -> <SolanaCrypto as ChainCrypto>::Payload {
		self.transaction.message().clone()
	}

	fn signed(mut self, signature: &<SolanaCrypto as ChainCrypto>::ThresholdSignature) -> Self {
		self.transaction.signatures = vec![*signature];
		self
	}

	fn chain_encoded(&self) -> Vec<u8> {
		self.transaction.clone().finalize_and_serialize().unwrap_or_default()
	}

	fn is_signed(&self) -> bool {
		self.transaction.is_signed()
	}

	fn transaction_out_id(&self) -> <SolanaCrypto as ChainCrypto>::TransactionOutId {
		todo!("Double check on the transaction out ID")
	}
}
