pub mod batch_fetch_and_transfer;
pub mod rotate_vault_proxy;

use super::{
	PolkadotAccountId, PolkadotCrypto, PolkadotExtrinsicBuilder, PolkadotPublicKey, RuntimeVersion,
};
use crate::{dot::Polkadot, *};
use frame_support::{traits::Get, CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound};
use sp_std::marker::PhantomData;

/// Chainflip api calls available on Polkadot.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub enum PolkadotApi<Environment: 'static> {
	BatchFetchAndTransfer(PolkadotExtrinsicBuilder),
	RotateVaultProxy(PolkadotExtrinsicBuilder),
	ChangeGovKey(PolkadotExtrinsicBuilder),
	ExecuteXSwapAndCall(PolkadotExtrinsicBuilder),
	#[doc(hidden)]
	#[codec(skip)]
	_Phantom(PhantomData<Environment>, Never),
}

pub trait PolkadotEnvironment {
	fn try_vault_account() -> Option<PolkadotAccountId>;
	fn vault_account() -> PolkadotAccountId {
		Self::try_vault_account().expect("Vault account must be set")
	}

	fn runtime_version() -> RuntimeVersion;
}

impl<T: ChainEnvironment<VaultAccount, PolkadotAccountId> + Get<RuntimeVersion>> PolkadotEnvironment
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

impl<E> ConsolidateCall<Polkadot> for PolkadotApi<E>
where
	E: PolkadotEnvironment + ReplayProtectionProvider<Polkadot>,
{
	fn consolidate_utxos() -> Result<Self, ConsolidationError> {
		Err(ConsolidationError::NotRequired)
	}
}

impl<E> AllBatch<Polkadot> for PolkadotApi<E>
where
	E: PolkadotEnvironment + ReplayProtectionProvider<Polkadot>,
{
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Polkadot>>,
		transfer_params: Vec<TransferAssetParams<Polkadot>>,
	) -> Result<Self, AllBatchError> {
		Ok(Self::BatchFetchAndTransfer(batch_fetch_and_transfer::extrinsic_builder(
			E::replay_protection(false),
			fetch_params,
			transfer_params,
			E::try_vault_account().ok_or(AllBatchError::VaultAccountNotSet)?,
		)))
	}
}

impl<E> SetGovKeyWithAggKey<PolkadotCrypto> for PolkadotApi<E>
where
	E: PolkadotEnvironment + ReplayProtectionProvider<Polkadot>,
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

impl<E> SetAggKeyWithAggKey<PolkadotCrypto> for PolkadotApi<E>
where
	E: PolkadotEnvironment + ReplayProtectionProvider<Polkadot>,
{
	fn new_unsigned(
		maybe_old_key: Option<PolkadotPublicKey>,
		new_key: PolkadotPublicKey,
	) -> Result<Self, SetAggKeyWithAggKeyError> {
		let vault = E::try_vault_account().ok_or(SetAggKeyWithAggKeyError::Failed)?;

		Ok(Self::RotateVaultProxy(rotate_vault_proxy::extrinsic_builder(
			// we reset the proxy account nonce on a rotation tx
			E::replay_protection(true),
			maybe_old_key,
			new_key,
			vault,
		)))
	}
}

impl<E> ExecutexSwapAndCall<Polkadot> for PolkadotApi<E>
where
	E: PolkadotEnvironment + ReplayProtectionProvider<Polkadot>,
{
	fn new_unsigned(
		_transfer_param: TransferAssetParams<Polkadot>,
		_source_chain: ForeignChain,
		_source_address: Option<ForeignChainAddress>,
		_gas_budget: <Polkadot as Chain>::ChainAmount,
		_message: Vec<u8>,
	) -> Result<Self, DispatchError> {
		Err(DispatchError::Other("Not implemented"))
	}
}

impl<E> TransferFallback<Polkadot> for PolkadotApi<E>
where
	E: PolkadotEnvironment + ReplayProtectionProvider<Polkadot>,
{
	fn new_unsigned(_transfer_param: TransferAssetParams<Polkadot>) -> Result<Self, DispatchError> {
		Err(DispatchError::Other("TransferFallback is not supported for the Polkadot chain."))
	}
}

macro_rules! map_over_api_variants {
	( $self:expr, $var:pat_param, $var_method:expr $(,)* ) => {
		match $self {
			PolkadotApi::BatchFetchAndTransfer($var) => $var_method,
			PolkadotApi::RotateVaultProxy($var) => $var_method,
			PolkadotApi::ChangeGovKey($var) => $var_method,
			PolkadotApi::ExecuteXSwapAndCall($var) => $var_method,
			PolkadotApi::_Phantom(..) => unreachable!(),
		}
	};
}

impl<E: PolkadotEnvironment> ApiCall<PolkadotCrypto> for PolkadotApi<E> {
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
	) -> Self {
		map_over_api_variants!(
			self,
			ref mut call,
			call.insert_signature(threshold_signature.clone())
		);
		self
	}

	fn chain_encoded(&self) -> Vec<u8> {
		map_over_api_variants!(
			self,
			call,
			call.get_signed_unchecked_extrinsic()
				.expect("Must be called after `signed`")
				.encode()
		)
	}

	fn is_signed(&self) -> bool {
		map_over_api_variants!(self, call, call.is_signed())
	}

	fn transaction_out_id(&self) -> <PolkadotCrypto as ChainCrypto>::TransactionOutId {
		map_over_api_variants!(self, call, call.signature().unwrap())
	}
}

pub trait CreatePolkadotVault: ApiCall<PolkadotCrypto> {
	fn new_unsigned() -> Self;
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, TypeInfo, Encode, Decode)]
#[scale_info(skip_type_params(E))]
pub struct OpaqueApiCall<E> {
	builder: PolkadotExtrinsicBuilder,
	#[codec(skip)]
	_environment: PhantomData<E>,
}

impl<E> From<OpaqueApiCall<E>> for PolkadotExtrinsicBuilder {
	fn from(call: OpaqueApiCall<E>) -> Self {
		call.builder
	}
}

trait WithEnvironment {
	fn with_environment<E>(self) -> OpaqueApiCall<E>;
}

impl WithEnvironment for PolkadotExtrinsicBuilder {
	fn with_environment<E>(self) -> OpaqueApiCall<E> {
		OpaqueApiCall { builder: self, _environment: PhantomData }
	}
}

impl<E: PolkadotEnvironment + 'static> ApiCall<PolkadotCrypto> for OpaqueApiCall<E> {
	fn threshold_signature_payload(&self) -> <PolkadotCrypto as ChainCrypto>::Payload {
		let RuntimeVersion { spec_version, transaction_version, .. } = E::runtime_version();

		self.builder.get_signature_payload(spec_version, transaction_version)
	}

	fn signed(mut self, signature: &<PolkadotCrypto as ChainCrypto>::ThresholdSignature) -> Self {
		self.builder.insert_signature(signature.clone());
		self
	}

	fn chain_encoded(&self) -> Vec<u8> {
		self.builder
			.get_signed_unchecked_extrinsic()
			.expect("Must be called after `signed`")
			.encode()
	}

	fn is_signed(&self) -> bool {
		self.builder.is_signed()
	}

	fn transaction_out_id(&self) -> <PolkadotCrypto as ChainCrypto>::TransactionOutId {
		self.builder.signature().unwrap()
	}
}
