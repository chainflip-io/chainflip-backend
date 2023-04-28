use codec::Encode;
use sp_core::Pair;
use sp_runtime::{
	generic::Era,
	traits::{IdentifyAccount, Verify},
	MultiAddress,
};
use sp_version::RuntimeVersion;
use state_chain_runtime::{AccountId, Signature};

/// A wrapper around a substrate [`Pair`] that can be used for signing.
#[derive(Clone, Debug)]
pub struct PairSigner<P: Pair> {
	pub account_id: AccountId,
	signer: P,
}

impl<P> PairSigner<P>
where
	Signature: From<P::Signature>,
	<Signature as Verify>::Signer: From<P::Public> + IdentifyAccount<AccountId = AccountId>,
	P: Pair,
{
	/// Creates a new [`Signer`] from a [`Pair`].
	pub fn new(signer: P) -> Self {
		let account_id = <Signature as Verify>::Signer::from(signer.public()).into_account();
		Self { account_id, signer }
	}

	/// Returns a signed extrinsic that matches the provided call
	pub fn new_signed_extrinsic(
		&self,
		call: state_chain_runtime::RuntimeCall,
		runtime_version: &RuntimeVersion,
		genesis_hash: state_chain_runtime::Hash,
		current_hash: state_chain_runtime::Hash,
		current_block_number: state_chain_runtime::BlockNumber,
		lifetime: state_chain_runtime::BlockNumber,
		nonce: state_chain_runtime::Index,
	) -> (
		state_chain_runtime::UncheckedExtrinsic,
		std::ops::RangeTo<state_chain_runtime::BlockNumber>,
	) {
		assert!(lifetime <= state_chain_runtime::BlockHashCount::get());

		let era = Era::mortal(lifetime as u64, current_block_number as u64);

		let lifetime = ..era.death(current_block_number as u64) as state_chain_runtime::BlockNumber;

		let extra: state_chain_runtime::SignedExtra = (
			frame_system::CheckNonZeroSender::new(),
			frame_system::CheckSpecVersion::new(),
			frame_system::CheckTxVersion::new(),
			frame_system::CheckGenesis::new(),
			frame_system::CheckEra::from(era),
			frame_system::CheckNonce::from(nonce),
			frame_system::CheckWeight::new(),
			// This is the tx fee tip. Normally this determines transaction priority. We currently
			// ignore this in the runtime but it needs to be set to some default value.
			state_chain_runtime::ChargeTransactionPayment::from(0),
		);
		let additional_signed = (
			(),
			runtime_version.spec_version,
			runtime_version.transaction_version,
			genesis_hash,
			current_hash,
			(),
			(),
			(),
		);

		let signed_payload = state_chain_runtime::SignedPayload::from_raw(
			call.clone(),
			extra.clone(),
			additional_signed,
		);
		let signature = signed_payload.using_encoded(|bytes| self.signer.sign(bytes).into());

		(
			state_chain_runtime::UncheckedExtrinsic::new_signed(
				call,
				MultiAddress::Id(self.account_id.clone()),
				signature,
				extra,
			),
			lifetime,
		)
	}
}
