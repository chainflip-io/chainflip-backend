// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

pub mod batch_fetch_and_transfer;
pub mod execute_x_swap_and_call;
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
use hub::{AssethubExtrinsicBuilder, AssethubRuntimeCall, OutputAccountId};

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
	fn get_new_output_channel_id() -> OutputAccountId;
}

impl<
		T: ChainEnvironment<VaultAccount, PolkadotAccountId>
			+ Get<RuntimeVersion>
			+ Get<OutputAccountId>,
	> AssethubEnvironment for T
{
	fn try_vault_account() -> Option<PolkadotAccountId> {
		Self::lookup(VaultAccount)
	}

	fn runtime_version() -> RuntimeVersion {
		Self::get()
	}

	fn get_new_output_channel_id() -> OutputAccountId {
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
		transfer_param: TransferAssetParams<Assethub>,
		_source_chain: ForeignChain,
		_source_address: Option<ForeignChainAddress>,
		_gas_budget: <Assethub as Chain>::ChainAmount,
		message: Vec<u8>,
		_ccm_additional_data: Vec<u8>,
	) -> Result<Self, ExecutexSwapAndCallError> {
		match <AssethubRuntimeCall as codec::Decode>::decode(&mut message.as_ref()) {
			Ok(xcm_call) => {
				let vault = E::try_vault_account().ok_or(ExecutexSwapAndCallError::NoVault)?;
				let output_channel_id = E::get_new_output_channel_id();
				Ok(Self::ExecuteXSwapAndCall(execute_x_swap_and_call::extrinsic_builder(
					E::replay_protection(false),
					output_channel_id,
					transfer_param,
					vault,
					xcm_call,
				)))
			},
			_ => Err(ExecutexSwapAndCallError::Unsupported),
		}
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

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Encode, Decode, PartialEq, Debug, TypeInfo, Eq)]
pub enum TransferType {
	/// should teleport `asset` to `dest`
	Teleport,
	/// should reserve-transfer `asset` to `dest`, using local chain as reserve
	LocalReserve,
	/// should reserve-transfer `asset` to `dest`, using `dest` as reserve
	DestinationReserve,
	/// should reserve-transfer `asset` to `dest`, using remote chain `Location` as reserve
	RemoteReserve(hub::xcm_types::hub_runtime_types::xcm::VersionedLocation),
}
