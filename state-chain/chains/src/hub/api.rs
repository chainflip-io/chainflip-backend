pub mod batch_fetch_and_transfer;
pub mod rotate_vault_proxy;

use crate::{
	dot::{PolkadotAccountId, PolkadotCrypto, PolkadotPublicKey, RuntimeVersion},
	hub::Assethub,
	*,
};
use frame_support::{
	traits::{Defensive, Get},
	CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound,
};
use hub::AssethubExtrinsicBuilder;

use sp_std::marker::PhantomData;

/// Chainflip api calls available on Assethub.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub enum AssethubApi<Environment: 'static> {
	BatchFetchAndTransfer(AssethubExtrinsicBuilder),
	RotateVaultProxy(AssethubExtrinsicBuilder),
	ChangeGovKey(AssethubExtrinsicBuilder),
	ExecuteXSwapAndCall(AssethubExtrinsicBuilder),
	#[doc(hidden)]
	#[codec(skip)]
	_Phantom(PhantomData<Environment>, Never),
}

pub trait AssethubEnvironment {
	fn try_vault_account() -> Option<PolkadotAccountId>;
	fn vault_account() -> PolkadotAccountId {
		Self::try_vault_account().expect("Vault account must be set")
	}

	fn runtime_version() -> RuntimeVersion;
}

impl<T: ChainEnvironment<VaultAccount, PolkadotAccountId> + Get<RuntimeVersion>> AssethubEnvironment
	for T
{
	fn try_vault_account() -> Option<PolkadotAccountId> {
		Self::lookup(VaultAccount)
	}

	fn runtime_version() -> RuntimeVersion {
		Self::get()
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct VaultAccount;

impl<E> ConsolidateCall<Assethub> for AssethubApi<E>
where
	E: AssethubEnvironment + ReplayProtectionProvider<Assethub>,
{
	fn consolidate_utxos() -> Result<Self, ConsolidationError> {
		Err(ConsolidationError::NotRequired)
	}
}

impl<E> AllBatch<Assethub> for AssethubApi<E>
where
	E: AssethubEnvironment + ReplayProtectionProvider<Assethub>,
{
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Assethub>>,
		transfer_params: Vec<(TransferAssetParams<Assethub>, EgressId)>,
	) -> Result<Vec<(Self, Vec<EgressId>)>, AllBatchError> {
		let (transfer_params, egress_ids) = transfer_params.into_iter().unzip();

		Ok(vec![(
			Self::BatchFetchAndTransfer(batch_fetch_and_transfer::extrinsic_builder(
				E::replay_protection(false),
				fetch_params,
				transfer_params,
				E::try_vault_account().ok_or(AllBatchError::VaultAccountNotSet)?,
			)),
			egress_ids,
		)])
	}
}

impl<E> SetGovKeyWithAggKey<PolkadotCrypto> for AssethubApi<E>
where
	E: AssethubEnvironment + ReplayProtectionProvider<Assethub>,
{
	fn new_unsigned(
		maybe_old_key: Option<PolkadotPublicKey>,
		new_key: PolkadotPublicKey,
	) -> Result<Self, ()> {
		let vault = E::try_vault_account().ok_or(())?;

		Ok(Self::ChangeGovKey(rotate_vault_proxy::extrinsic_builder(
			E::replay_protection(false),
			maybe_old_key,
			new_key,
			vault,
		)))
	}
}

impl<E> SetAggKeyWithAggKey<PolkadotCrypto> for AssethubApi<E>
where
	E: AssethubEnvironment + ReplayProtectionProvider<Assethub>,
{
	fn new_unsigned(
		maybe_old_key: Option<PolkadotPublicKey>,
		new_key: PolkadotPublicKey,
	) -> Result<Option<Self>, SetAggKeyWithAggKeyError> {
		let vault = E::try_vault_account().ok_or(SetAggKeyWithAggKeyError::Failed)?;

		Ok(Some(Self::RotateVaultProxy(rotate_vault_proxy::extrinsic_builder(
			// we reset the proxy account nonce on a rotation tx
			E::replay_protection(true),
			maybe_old_key,
			new_key,
			vault,
		))))
	}
}

impl<E> ExecutexSwapAndCall<Assethub> for AssethubApi<E>
where
	E: AssethubEnvironment + ReplayProtectionProvider<Assethub>,
{
	fn new_unsigned(
		_transfer_param: TransferAssetParams<Assethub>,
		_source_chain: ForeignChain,
		_source_address: Option<ForeignChainAddress>,
		_gas_budget: <Assethub as Chain>::ChainAmount,
		_message: Vec<u8>,
		_ccm_additional_data: Vec<u8>,
	) -> Result<Self, ExecutexSwapAndCallError> {
		Err(ExecutexSwapAndCallError::Unsupported)
	}
}

impl<E> TransferFallback<Assethub> for AssethubApi<E>
where
	E: AssethubEnvironment + ReplayProtectionProvider<Assethub>,
{
	fn new_unsigned(
		_transfer_param: TransferAssetParams<Assethub>,
	) -> Result<Self, TransferFallbackError> {
		Err(TransferFallbackError::Unsupported)
	}
}

impl<E> RejectCall<Assethub> for AssethubApi<E> where
	E: AssethubEnvironment + ReplayProtectionProvider<Assethub>
{
}

macro_rules! map_over_api_variants {
	( $self:expr, $var:pat_param, $var_method:expr $(,)* ) => {
		match $self {
			AssethubApi::BatchFetchAndTransfer($var) => $var_method,
			AssethubApi::RotateVaultProxy($var) => $var_method,
			AssethubApi::ChangeGovKey($var) => $var_method,
			AssethubApi::ExecuteXSwapAndCall($var) => $var_method,
			AssethubApi::_Phantom(..) => unreachable!(),
		}
	};
}

impl<E: AssethubEnvironment + ReplayProtectionProvider<Assethub>> ApiCall<PolkadotCrypto>
	for AssethubApi<E>
{
	fn threshold_signature_payload(&self) -> <PolkadotCrypto as ChainCrypto>::Payload {
		let RuntimeVersion { spec_version, transaction_version, .. } = E::runtime_version();
		map_over_api_variants!(
			self,
			call,
			call.get_signature_payload(spec_version, transaction_version)
		)
	}

	fn signed(
		mut self,
		threshold_signature: &<PolkadotCrypto as ChainCrypto>::ThresholdSignature,
		signer: <PolkadotCrypto as ChainCrypto>::AggKey,
	) -> Self {
		map_over_api_variants!(
			self,
			ref mut call,
			call.insert_signer_and_signature(signer, threshold_signature.clone())
		);
		self
	}

	fn chain_encoded(&self) -> Vec<u8> {
		map_over_api_variants!(
			self,
			call,
			call.get_signed_unchecked_extrinsic()
				.defensive_proof("`chain_encoded` is only called on signed api calls.")
				.map(|extrinsic| extrinsic.encode())
				.unwrap_or_default()
		)
	}

	fn is_signed(&self) -> bool {
		map_over_api_variants!(self, call, call.is_signed())
	}

	fn transaction_out_id(&self) -> <PolkadotCrypto as ChainCrypto>::TransactionOutId {
		map_over_api_variants!(self, call, call.signature().unwrap())
	}

	fn refresh_replay_protection(&mut self) {
		map_over_api_variants!(
			self,
			call,
			call.refresh_replay_protection(E::replay_protection(false))
		)
	}

	fn signer(&self) -> Option<<PolkadotCrypto as ChainCrypto>::AggKey> {
		map_over_api_variants!(self, call, call.signer_and_signature.clone())
			.map(|(signer, _)| signer)
	}
}
