use crate::subxt_state_chain_config::StateChainConfig;
use codec::Encode;
use sp_api::__private::RuntimeVersion;
use sp_core::Pair;
use sp_runtime::{
	generic::Era,
	traits::{IdentifyAccount, Verify},
	MultiAddress,
};
use state_chain_runtime::{AccountId, Signature};

pub mod broker_crypto {
	use sp_application_crypto::{app_crypto, sr25519, KeyTypeId};
	/// Broker Key Type ID used to store the key on state chain node keystore
	pub const BROKER_ID_KEY: KeyTypeId = KeyTypeId(*b"brok");

	app_crypto!(sr25519, BROKER_ID_KEY);
}

pub struct SubxtSignerInterface<T>(subxt::utils::AccountId32, T);

impl<P> SubxtSignerInterface<P>
where
	Signature: From<P::Signature>,
	<Signature as Verify>::Signer: From<P::Public> + IdentifyAccount<AccountId = AccountId>,
	P: sp_core::Pair,
{
	pub fn new(pair: P) -> Self {
		let account_id = <Signature as Verify>::Signer::from(pair.public()).into_account();
		let bytes: &[u8; 32] = account_id.as_ref();
		Self(subxt::utils::AccountId32::from(*bytes), pair)
	}

	pub fn account(&self) -> state_chain_runtime::AccountId {
		<Signature as Verify>::Signer::from(self.1.public()).into_account()
	}
}

impl subxt::tx::Signer<StateChainConfig> for SubxtSignerInterface<sp_core::sr25519::Pair> {
	fn account_id(&self) -> <StateChainConfig as subxt::Config>::AccountId {
		self.0.clone()
	}

	fn address(&self) -> <StateChainConfig as subxt::Config>::Address {
		subxt::utils::MultiAddress::Id(self.0.clone())
	}

	fn sign(&self, bytes: &[u8]) -> <StateChainConfig as subxt::Config>::Signature {
		use sp_core::Pair;
		state_chain_runtime::Signature::Sr25519(self.1.sign(bytes))
	}
}

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
		nonce: state_chain_runtime::Nonce,
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
			frame_metadata_hash_extension::CheckMetadataHash::<state_chain_runtime::Runtime>::new(
				false,
			),
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
			None,
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
