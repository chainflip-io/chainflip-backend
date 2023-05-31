pub mod batch_fetch_and_transfer;
pub mod create_anonymous_vault;
pub mod rotate_vault_proxy;

use super::{PolkadotAccountId, PolkadotExtrinsicBuilder, PolkadotPublicKey, RuntimeVersion};
use crate::{dot::Polkadot, *};
use frame_support::{traits::Get, CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound};
use sp_std::marker::PhantomData;

/// Chainflip api calls available on Polkadot.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub enum PolkadotApi<Environment: 'static> {
	BatchFetchAndTransfer(PolkadotExtrinsicBuilder),
	RotateVaultProxy(PolkadotExtrinsicBuilder),
	CreateAnonymousVault(PolkadotExtrinsicBuilder),
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

	fn try_proxy_account() -> Option<PolkadotAccountId>;
	fn proxy_account() -> PolkadotAccountId {
		Self::try_proxy_account().expect("Proxy account must be set")
	}

	fn runtime_version() -> RuntimeVersion;
}

impl<T: ChainEnvironment<SystemAccounts, PolkadotAccountId> + Get<RuntimeVersion>>
	PolkadotEnvironment for T
{
	fn try_vault_account() -> Option<PolkadotAccountId> {
		Self::lookup(SystemAccounts::Vault)
	}

	fn try_proxy_account() -> Option<PolkadotAccountId> {
		Self::lookup(SystemAccounts::Proxy)
	}

	fn runtime_version() -> RuntimeVersion {
		Self::get()
	}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum SystemAccounts {
	Proxy,
	Vault,
}

impl<E> AllBatch<Polkadot> for PolkadotApi<E>
where
	E: PolkadotEnvironment + ReplayProtectionProvider<Polkadot>,
{
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Polkadot>>,
		transfer_params: Vec<TransferAssetParams<Polkadot>>,
	) -> Result<Self, ()> {
		Ok(Self::BatchFetchAndTransfer(batch_fetch_and_transfer::extrinsic_builder(
			E::replay_protection(),
			fetch_params,
			transfer_params,
			E::try_vault_account().ok_or(())?,
		)))
	}
}

impl<E> SetGovKeyWithAggKey<Polkadot> for PolkadotApi<E>
where
	E: PolkadotEnvironment + ReplayProtectionProvider<Polkadot>,
{
	fn new_unsigned(
		maybe_old_key: Option<PolkadotPublicKey>,
		new_key: PolkadotPublicKey,
	) -> Result<Self, ()> {
		let vault = E::try_vault_account().ok_or(())?;

		Ok(Self::ChangeGovKey(rotate_vault_proxy::extrinsic_builder(
			E::replay_protection(),
			maybe_old_key,
			new_key,
			vault,
		)))
	}
}

impl<E> SetAggKeyWithAggKey<Polkadot> for PolkadotApi<E>
where
	E: PolkadotEnvironment + ReplayProtectionProvider<Polkadot>,
{
	fn new_unsigned(
		maybe_old_key: Option<PolkadotPublicKey>,
		new_key: PolkadotPublicKey,
	) -> Result<Self, ()> {
		let vault = E::try_vault_account().ok_or(())?;

		Ok(Self::RotateVaultProxy(rotate_vault_proxy::extrinsic_builder(
			E::replay_protection(),
			maybe_old_key,
			new_key,
			vault,
		)))
	}
}

impl<E> CreatePolkadotVault for PolkadotApi<E>
where
	E: PolkadotEnvironment + ReplayProtectionProvider<Polkadot>,
{
	fn new_unsigned() -> Self {
		Self::CreateAnonymousVault(
			create_anonymous_vault::extrinsic_builder(E::replay_protection()),
		)
	}
}

impl<E> ExecutexSwapAndCall<Polkadot> for PolkadotApi<E>
where
	E: PolkadotEnvironment + ReplayProtectionProvider<Polkadot>,
{
	fn new_unsigned(
		_egress_id: EgressId,
		_transfer_param: TransferAssetParams<Polkadot>,
		_source_address: ForeignChainAddress,
		_message: Vec<u8>,
	) -> Result<Self, DispatchError> {
		Err(DispatchError::Other("Not implemented"))
	}
}

macro_rules! map_over_api_variants {
	( $self:expr, $var:pat_param, $var_method:expr $(,)* ) => {
		match $self {
			PolkadotApi::BatchFetchAndTransfer($var) => $var_method,
			PolkadotApi::RotateVaultProxy($var) => $var_method,
			PolkadotApi::CreateAnonymousVault($var) => $var_method,
			PolkadotApi::ChangeGovKey($var) => $var_method,
			PolkadotApi::ExecuteXSwapAndCall($var) => $var_method,
			PolkadotApi::_Phantom(..) => unreachable!(),
		}
	};
}

impl<E: PolkadotEnvironment> ApiCall<Polkadot> for PolkadotApi<E> {
	fn threshold_signature_payload(&self) -> <Polkadot as ChainCrypto>::Payload {
		let RuntimeVersion { spec_version, transaction_version, .. } = E::runtime_version();
		map_over_api_variants!(
			self,
			call,
			call.get_signature_payload(spec_version, transaction_version)
		)
	}

	fn signed(
		mut self,
		threshold_signature: &<Polkadot as ChainCrypto>::ThresholdSignature,
	) -> Self {
		let proxy_account = E::proxy_account();
		map_over_api_variants!(
			self,
			ref mut call,
			call.insert_signature(proxy_account, threshold_signature.clone())
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

	fn transaction_out_id(&self) -> <Polkadot as ChainCrypto>::TransactionOutId {
		map_over_api_variants!(self, call, call.signature().unwrap())
	}
}

pub trait CreatePolkadotVault: ApiCall<Polkadot> {
	fn new_unsigned() -> Self;
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, TypeInfo, Encode, Decode)]
#[scale_info(skip_type_params(E))]
pub struct OpaqueApiCall<E> {
	builder: PolkadotExtrinsicBuilder,
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

impl<E: PolkadotEnvironment + 'static> ApiCall<Polkadot> for OpaqueApiCall<E> {
	fn threshold_signature_payload(&self) -> <Polkadot as ChainCrypto>::Payload {
		let RuntimeVersion { spec_version, transaction_version, .. } = E::runtime_version();

		self.builder.get_signature_payload(spec_version, transaction_version)
	}

	fn signed(mut self, signature: &<Polkadot as ChainCrypto>::ThresholdSignature) -> Self {
		self.builder.insert_signature(E::proxy_account(), signature.clone());
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

	fn transaction_out_id(&self) -> <Polkadot as ChainCrypto>::TransactionOutId {
		self.builder.signature().unwrap()
	}
}

#[cfg(test)]
mod mocks {
	use super::*;
	use crate::dot::{PolkadotPair, PolkadotReplayProtection, NONCE_1, RAW_SEED_1, RAW_SEED_2};

	pub struct MockEnv;

	impl PolkadotEnvironment for MockEnv {
		fn try_vault_account() -> Option<PolkadotAccountId> {
			Some(PolkadotPair::from_seed(&RAW_SEED_1).public_key())
		}

		fn try_proxy_account() -> Option<PolkadotAccountId> {
			Some(PolkadotPair::from_seed(&RAW_SEED_2).public_key())
		}

		fn runtime_version() -> crate::dot::RuntimeVersion {
			dot::TEST_RUNTIME_VERSION
		}
	}

	impl ReplayProtectionProvider<Polkadot> for MockEnv {
		fn replay_protection() -> PolkadotReplayProtection {
			PolkadotReplayProtection { nonce: NONCE_1, genesis_hash: Default::default() }
		}
	}
}
