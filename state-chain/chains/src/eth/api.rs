use super::Ethereum;
use crate::{
	evm::{
		api::{
			all_batch, execute_x_swap_and_call, set_agg_key_with_agg_key,
			set_comm_key_with_agg_key, set_gov_key_with_agg_key, EthEnvironmentProvider,
			EthereumCall, EthereumTransactionBuilder, EvmReplayProtection, SigData,
		},
		EvmFetchId, SchnorrVerificationComponents,
	},
	*,
};
use ethabi::{Address, Uint};
use evm::api::common::*;
use frame_support::{
	sp_runtime::{
		traits::{Hash, Keccak256, UniqueSaturatedInto},
		DispatchError,
	},
	CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound,
};
use sp_std::marker::PhantomData;

use evm::tokenizable::Tokenizable;

#[cfg(feature = "std")]
pub mod abi {
	#[macro_export]
	macro_rules! include_abi_bytes {
		($name:ident) => {
			&include_bytes!(concat!(
				env!("CF_ETH_CONTRACT_ABI_ROOT"),
				"/",
				env!("CF_ETH_CONTRACT_ABI_TAG"),
				"/",
				stringify!($name),
				".json"
			))[..]
		};
	}

	#[cfg(test)]
	pub fn load_abi(name: &'static str) -> ethabi::Contract {
		fn abi_file(name: &'static str) -> std::path::PathBuf {
			let mut path = std::path::PathBuf::from(env!("CF_ETH_CONTRACT_ABI_ROOT"));
			path.push(env!("CF_ETH_CONTRACT_ABI_TAG"));
			path.push(name);
			path.set_extension("json");
			path.canonicalize()
				.unwrap_or_else(|e| panic!("Failed to canonicalize abi file {path:?}: {e}"))
		}

		fn load_abi_bytes(name: &'static str) -> impl std::io::Read {
			std::fs::File::open(abi_file(name))
				.unwrap_or_else(|e| panic!("Failed to open abi file {:?}: {e}", abi_file(name)))
		}

		ethabi::Contract::load(load_abi_bytes(name)).expect("Failed to load abi from bytes.")
	}
}

pub mod register_redemption;
pub mod update_flip_supply;

/// Chainflip api calls available on Ethereum.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub enum EthereumApi<Environment: 'static> {
	SetAggKeyWithAggKey(EthereumTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>),
	RegisterRedemption(EthereumTransactionBuilder<register_redemption::RegisterRedemption>),
	UpdateFlipSupply(EthereumTransactionBuilder<update_flip_supply::UpdateFlipSupply>),
	SetGovKeyWithAggKey(EthereumTransactionBuilder<set_gov_key_with_agg_key::SetGovKeyWithAggKey>),
	SetCommKeyWithAggKey(
		EthereumTransactionBuilder<set_comm_key_with_agg_key::SetCommKeyWithAggKey>,
	),
	AllBatch(EthereumTransactionBuilder<all_batch::AllBatch>),
	ExecutexSwapAndCall(EthereumTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>),
	#[doc(hidden)]
	#[codec(skip)]
	_Phantom(PhantomData<Environment>, Never),
}

impl<C: EthereumCall + Parameter + 'static> ApiCall<Ethereum> for EthereumTransactionBuilder<C> {
	fn threshold_signature_payload(
		&self,
	) -> <<Ethereum as Chain>::ChainCrypto as ChainCrypto>::Payload {
		Keccak256::hash(&ethabi::encode(&[
			self.call.msg_hash().tokenize(),
			self.replay_protection.tokenize(),
		]))
	}

	fn signed(
		mut self,
		threshold_signature: &<<Ethereum as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature,
	) -> Self {
		self.sig_data = Some(SigData::new(self.replay_protection.nonce, threshold_signature));
		self
	}

	fn chain_encoded(&self) -> Vec<u8> {
		self.call
			.abi_encoded(&self.sig_data.expect("Unsigned chain encoding is invalid."))
	}

	fn is_signed(&self) -> bool {
		self.sig_data.is_some()
	}

	fn transaction_out_id(
		&self,
	) -> <<Ethereum as Chain>::ChainCrypto as ChainCrypto>::TransactionOutId {
		let sig_data = self.sig_data.expect("Unsigned transaction_out_id is invalid.");
		SchnorrVerificationComponents {
			s: sig_data.sig.into(),
			k_times_g_address: sig_data.k_times_g_address.into(),
		}
	}
}

impl<E> SetAggKeyWithAggKey<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider + ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned(
		_old_key: Option<<<Ethereum as Chain>::ChainCrypto as ChainCrypto>::AggKey>,
		new_key: <<Ethereum as Chain>::ChainCrypto as ChainCrypto>::AggKey,
	) -> Result<Self, SetAggKeyWithAggKeyError> {
		Ok(Self::SetAggKeyWithAggKey(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(E::contract_address(EthereumContract::KeyManager)),
			set_agg_key_with_agg_key::SetAggKeyWithAggKey::new(new_key),
		)))
	}
}

impl<E> SetGovKeyWithAggKey<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider + ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned(
		_maybe_old_key: Option<<<Ethereum as Chain>::ChainCrypto as ChainCrypto>::GovKey>,
		new_gov_key: <<Ethereum as Chain>::ChainCrypto as ChainCrypto>::GovKey,
	) -> Result<Self, ()> {
		Ok(Self::SetGovKeyWithAggKey(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(E::contract_address(EthereumContract::KeyManager)),
			set_gov_key_with_agg_key::SetGovKeyWithAggKey::new(new_gov_key),
		)))
	}
}

impl<E> SetCommKeyWithAggKey<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider + ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned(
		new_comm_key: <<Ethereum as Chain>::ChainCrypto as ChainCrypto>::GovKey,
	) -> Self {
		Self::SetCommKeyWithAggKey(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(E::contract_address(EthereumContract::KeyManager)),
			set_comm_key_with_agg_key::SetCommKeyWithAggKey::new(new_comm_key),
		))
	}
}

impl<E> RegisterRedemption<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider + ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned(
		node_id: &[u8; 32],
		amount: u128,
		address: &[u8; 20],
		expiry: u64,
		executor: Option<Address>,
	) -> Self {
		Self::RegisterRedemption(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(E::contract_address(EthereumContract::StateChainGateway)),
			register_redemption::RegisterRedemption::new(
				node_id, amount, address, expiry, executor,
			),
		))
	}

	fn amount(&self) -> u128 {
		match self {
			EthereumApi::RegisterRedemption(tx_builder) =>
				tx_builder.call.amount.unique_saturated_into(),
			_ => unreachable!(),
		}
	}
}

impl<E> UpdateFlipSupply<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider + ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned(new_total_supply: u128, block_number: u64) -> Self {
		Self::UpdateFlipSupply(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(E::contract_address(EthereumContract::StateChainGateway)),
			update_flip_supply::UpdateFlipSupply::new(new_total_supply, block_number),
		))
	}
}

impl<E> AllBatch<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider + ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Ethereum>>,
		transfer_params: Vec<TransferAssetParams<Ethereum>>,
	) -> Result<Self, AllBatchError> {
		let mut fetch_only_params = vec![];
		let mut fetch_deploy_params = vec![];
		for FetchAssetParams { deposit_fetch_id, asset } in fetch_params {
			if let Some(token_address) = E::token_address(asset) {
				match deposit_fetch_id {
					EvmFetchId::Fetch(contract_address) => {
						debug_assert!(
							asset != assets::eth::Asset::Eth,
							"Eth should not be fetched. It is auto-fetched in the smart contract."
						);
						fetch_only_params.push(EncodableFetchAssetParams {
							contract_address,
							asset: token_address,
						})
					},
					EvmFetchId::DeployAndFetch(channel_id) => fetch_deploy_params
						.push(EncodableFetchDeployAssetParams { channel_id, asset: token_address }),
					EvmFetchId::NotRequired => (),
				};
			} else {
				return Err(AllBatchError::Other)
			}
		}
		Ok(Self::AllBatch(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(E::contract_address(EthereumContract::Vault)),
			all_batch::AllBatch::new(
				fetch_deploy_params,
				fetch_only_params,
				transfer_params
					.into_iter()
					.map(|TransferAssetParams { asset, to, amount }| {
						E::token_address(asset)
							.map(|address| EncodableTransferAssetParams {
								to,
								amount,
								asset: address,
							})
							.ok_or(AllBatchError::Other)
					})
					.collect::<Result<Vec<_>, _>>()?,
			),
		)))
	}
}

impl<E> ExecutexSwapAndCall<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider + ReplayProtectionProvider<Ethereum>,
{
	fn new_unsigned(
		egress_id: EgressId,
		transfer_param: TransferAssetParams<Ethereum>,
		source_chain: ForeignChain,
		source_address: Option<ForeignChainAddress>,
		message: Vec<u8>,
	) -> Result<Self, DispatchError> {
		let transfer_param = EncodableTransferAssetParams {
			asset: E::token_address(transfer_param.asset).ok_or(DispatchError::CannotLookup)?,
			to: transfer_param.to,
			amount: transfer_param.amount,
		};

		Ok(Self::ExecutexSwapAndCall(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(E::contract_address(EthereumContract::Vault)),
			execute_x_swap_and_call::ExecutexSwapAndCall::new(
				egress_id,
				transfer_param,
				source_chain,
				source_address,
				message,
			),
		)))
	}
}

impl<E> From<EthereumTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>>
	for EthereumApi<E>
{
	fn from(tx: EthereumTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>) -> Self {
		Self::SetAggKeyWithAggKey(tx)
	}
}

impl<E> From<EthereumTransactionBuilder<register_redemption::RegisterRedemption>>
	for EthereumApi<E>
{
	fn from(tx: EthereumTransactionBuilder<register_redemption::RegisterRedemption>) -> Self {
		Self::RegisterRedemption(tx)
	}
}

impl<E> From<EthereumTransactionBuilder<update_flip_supply::UpdateFlipSupply>> for EthereumApi<E> {
	fn from(tx: EthereumTransactionBuilder<update_flip_supply::UpdateFlipSupply>) -> Self {
		Self::UpdateFlipSupply(tx)
	}
}

impl<E> From<EthereumTransactionBuilder<set_gov_key_with_agg_key::SetGovKeyWithAggKey>>
	for EthereumApi<E>
{
	fn from(tx: EthereumTransactionBuilder<set_gov_key_with_agg_key::SetGovKeyWithAggKey>) -> Self {
		Self::SetGovKeyWithAggKey(tx)
	}
}

impl<E> From<EthereumTransactionBuilder<set_comm_key_with_agg_key::SetCommKeyWithAggKey>>
	for EthereumApi<E>
{
	fn from(
		tx: EthereumTransactionBuilder<set_comm_key_with_agg_key::SetCommKeyWithAggKey>,
	) -> Self {
		Self::SetCommKeyWithAggKey(tx)
	}
}

impl<E> From<EthereumTransactionBuilder<all_batch::AllBatch>> for EthereumApi<E> {
	fn from(tx: EthereumTransactionBuilder<all_batch::AllBatch>) -> Self {
		Self::AllBatch(tx)
	}
}

impl<E> From<EthereumTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>>
	for EthereumApi<E>
{
	fn from(tx: EthereumTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>) -> Self {
		Self::ExecutexSwapAndCall(tx)
	}
}

macro_rules! map_over_api_variants {
	( $self:expr, $var:pat_param, $var_method:expr $(,)* ) => {
		match $self {
			EthereumApi::SetAggKeyWithAggKey($var) => $var_method,
			EthereumApi::RegisterRedemption($var) => $var_method,
			EthereumApi::UpdateFlipSupply($var) => $var_method,
			EthereumApi::SetGovKeyWithAggKey($var) => $var_method,
			EthereumApi::SetCommKeyWithAggKey($var) => $var_method,
			EthereumApi::AllBatch($var) => $var_method,
			EthereumApi::ExecutexSwapAndCall($var) => $var_method,
			EthereumApi::_Phantom(..) => unreachable!(),
		}
	};
}

impl<E> EthereumApi<E> {
	pub fn replay_protection(&self) -> EvmReplayProtection {
		map_over_api_variants!(self, call, call.replay_protection())
	}
}

impl<E> ApiCall<Ethereum> for EthereumApi<E> {
	fn threshold_signature_payload(
		&self,
	) -> <<Ethereum as Chain>::ChainCrypto as ChainCrypto>::Payload {
		map_over_api_variants!(self, call, call.threshold_signature_payload())
	}

	fn signed(
		self,
		threshold_signature: &<<Ethereum as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature,
	) -> Self {
		map_over_api_variants!(self, call, call.signed(threshold_signature).into())
	}

	fn chain_encoded(&self) -> Vec<u8> {
		map_over_api_variants!(self, call, call.chain_encoded())
	}

	fn is_signed(&self) -> bool {
		map_over_api_variants!(self, call, call.is_signed())
	}

	fn transaction_out_id(
		&self,
	) -> <<Ethereum as Chain>::ChainCrypto as ChainCrypto>::TransactionOutId {
		map_over_api_variants!(self, call, call.transaction_out_id())
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum EthereumContract {
	StateChainGateway,
	KeyManager,
	Vault,
}
