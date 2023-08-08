use super::{Ethereum, EthereumFetchId, SchnorrVerificationComponents};
use crate::*;
use common::*;
use ethabi::{Address, ParamType, Token, Uint};
use frame_support::{
	sp_runtime::{
		traits::{Hash, Keccak256, UniqueSaturatedInto},
		DispatchError,
	},
	CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound,
};
use sp_std::marker::PhantomData;

pub use tokenizable::Tokenizable;

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

pub mod all_batch;
pub mod common;
pub mod execute_x_swap_and_call;
pub mod register_redemption;
pub mod set_agg_key_with_agg_key;
pub mod set_comm_key_with_agg_key;
pub mod set_gov_key_with_agg_key;
pub mod tokenizable;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Default)]
pub struct EthereumReplayProtection {
	pub nonce: u64,
	pub chain_id: EthereumChainId,
	pub key_manager_address: Address,
	pub contract_address: Address,
}

impl Tokenizable for EthereumReplayProtection {
	fn tokenize(self) -> Token {
		Token::FixedArray(vec![
			Token::Uint(Uint::from(self.nonce)),
			Token::Address(self.contract_address),
			Token::Uint(Uint::from(self.chain_id)),
			Token::Address(self.key_manager_address),
		])
	}

	fn param_type() -> ethabi::ParamType {
		ParamType::Tuple(vec![
			ParamType::Uint(256),
			ParamType::Address,
			ParamType::Uint(256),
			ParamType::Address,
		])
	}
}

/// The `SigData` struct used for threshold signatures in the smart contracts.
/// See [here](https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/interfaces/IShared.sol).
#[derive(
	Encode,
	Decode,
	TypeInfo,
	Copy,
	Clone,
	RuntimeDebug,
	PartialEq,
	Eq,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub struct SigData {
	/// The Schnorr signature.
	sig: Uint,
	/// The nonce value for the AggKey. Each Signature over an AggKey should have a unique
	/// nonce to prevent replay attacks.
	pub nonce: Uint,
	/// The address value derived from the random nonce value `k`. Also known as
	/// `nonceTimesGeneratorAddress`.
	///
	/// Note this is unrelated to the `nonce` above. The nonce in the context of
	/// `nonceTimesGeneratorAddress` is a generated as part of each signing round (ie. as part
	/// of the Schnorr signature) to prevent certain classes of cryptographic attacks.
	k_times_g_address: Address,
}

impl SigData {
	/// Add the actual signature. This method does no verification.
	pub fn new(nonce: impl Into<Uint>, schnorr: &SchnorrVerificationComponents) -> Self {
		Self {
			sig: schnorr.s.into(),
			nonce: nonce.into(),
			k_times_g_address: schnorr.k_times_g_address.into(),
		}
	}
}

impl Tokenizable for SigData {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![
			self.sig.tokenize(),
			self.nonce.tokenize(),
			self.k_times_g_address.tokenize(),
		])
	}

	fn param_type() -> ParamType {
		ParamType::Tuple(vec![ParamType::Uint(256), ParamType::Uint(256), ParamType::Address])
	}
}

pub trait EthereumCall {
	const FUNCTION_NAME: &'static str;

	/// The function names and parameters, not including sigData.
	fn function_params() -> Vec<(&'static str, ethabi::ParamType)>;
	/// The function values to be used as call parameters, no including sigData.
	fn function_call_args(&self) -> Vec<Token>;

	fn get_function() -> ethabi::Function {
		#[allow(deprecated)]
		ethabi::Function {
			name: Self::FUNCTION_NAME.into(),
			inputs: core::iter::once(("sigData", SigData::param_type()))
				.chain(Self::function_params())
				.map(|(n, t)| ethabi_param(n, t))
				.collect(),
			outputs: vec![],
			constant: None,
			state_mutability: ethabi::StateMutability::NonPayable,
		}
	}
	/// Encodes the call and signature into Ethereum Abi format.
	fn abi_encoded(&self, sig_data: &SigData) -> Vec<u8> {
		Self::get_function()
			.encode_input(
				&core::iter::once(sig_data.tokenize())
					.chain(self.function_call_args())
					.collect::<Vec<_>>(),
			)
			.expect(
				r#"
					This can only fail if the parameter types don't match the function signature.
					Therefore, as long as the tests pass, it can't fail at runtime.
				"#,
			)
	}
	/// Generates the message hash for this call.
	fn msg_hash(&self) -> <Keccak256 as Hash>::Output {
		Keccak256::hash(&ethabi::encode(
			&core::iter::once(Self::get_function().tokenize())
				.chain(self.function_call_args())
				.collect::<Vec<_>>(),
		))
	}
}

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct EthereumTransactionBuilder<C> {
	sig_data: Option<SigData>,
	replay_protection: EthereumReplayProtection,
	call: C,
}

impl<C: EthereumCall> EthereumTransactionBuilder<C> {
	pub fn new_unsigned(replay_protection: EthereumReplayProtection, call: C) -> Self {
		Self { replay_protection, call, sig_data: None }
	}

	pub fn replay_protection(&self) -> EthereumReplayProtection {
		self.replay_protection
	}

	pub fn chain_id(&self) -> EthereumChainId {
		self.replay_protection.chain_id
	}
}

impl<C: EthereumCall + Parameter + 'static> ApiCall<Ethereum> for EthereumTransactionBuilder<C> {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		Keccak256::hash(&ethabi::encode(&[
			self.call.msg_hash().tokenize(),
			self.replay_protection.tokenize(),
		]))
	}

	fn signed(
		mut self,
		threshold_signature: &<Ethereum as ChainCrypto>::ThresholdSignature,
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

	fn transaction_out_id(&self) -> <Ethereum as ChainCrypto>::TransactionOutId {
		let sig_data = self.sig_data.expect("Unsigned transaction_out_id is invalid.");
		SchnorrVerificationComponents {
			s: sig_data.sig.into(),
			k_times_g_address: sig_data.k_times_g_address.into(),
		}
	}
}

/// Provides the environment data for ethereum-like chains.
pub trait EthEnvironmentProvider {
	fn token_address(asset: assets::eth::Asset) -> Option<eth::Address>;
	fn contract_address(contract: EthereumContract) -> eth::Address;
	fn chain_id() -> EthereumChainId;
	fn next_nonce() -> u64;

	fn replay_protection(contract: EthereumContract) -> EthereumReplayProtection {
		EthereumReplayProtection {
			nonce: Self::next_nonce(),
			chain_id: Self::chain_id(),
			key_manager_address: Self::key_manager_address(),
			contract_address: Self::contract_address(contract),
		}
	}

	fn key_manager_address() -> eth::Address {
		Self::contract_address(EthereumContract::KeyManager)
	}

	fn state_chain_gateway_address() -> eth::Address {
		Self::contract_address(EthereumContract::StateChainGateway)
	}

	fn vault_address() -> eth::Address {
		Self::contract_address(EthereumContract::Vault)
	}
}

impl ChainAbi for Ethereum {
	type Transaction = eth::Transaction;
	type ReplayProtection = EthereumReplayProtection;
}

impl<E> SetAggKeyWithAggKey<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider,
{
	fn new_unsigned(
		_old_key: Option<<Ethereum as ChainCrypto>::AggKey>,
		new_key: <Ethereum as ChainCrypto>::AggKey,
	) -> Result<Self, SetAggKeyWithAggKeyError> {
		Ok(Self::SetAggKeyWithAggKey(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(EthereumContract::KeyManager),
			set_agg_key_with_agg_key::SetAggKeyWithAggKey::new(new_key),
		)))
	}
}

impl<E> SetGovKeyWithAggKey<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider,
{
	fn new_unsigned(
		_maybe_old_key: Option<<Ethereum as ChainCrypto>::GovKey>,
		new_gov_key: <Ethereum as ChainCrypto>::GovKey,
	) -> Result<Self, ()> {
		Ok(Self::SetGovKeyWithAggKey(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(EthereumContract::KeyManager),
			set_gov_key_with_agg_key::SetGovKeyWithAggKey::new(new_gov_key),
		)))
	}
}

impl<E> SetCommKeyWithAggKey<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider,
{
	fn new_unsigned(new_comm_key: <Ethereum as ChainCrypto>::GovKey) -> Self {
		Self::SetCommKeyWithAggKey(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(EthereumContract::KeyManager),
			set_comm_key_with_agg_key::SetCommKeyWithAggKey::new(new_comm_key),
		))
	}
}

impl<E> RegisterRedemption<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider,
{
	fn new_unsigned(node_id: &[u8; 32], amount: u128, address: &[u8; 20], expiry: u64) -> Self {
		Self::RegisterRedemption(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(EthereumContract::StateChainGateway),
			register_redemption::RegisterRedemption::new(node_id, amount, address, expiry),
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
	E: EthEnvironmentProvider,
{
	fn new_unsigned(new_total_supply: u128, block_number: u64) -> Self {
		Self::UpdateFlipSupply(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(EthereumContract::StateChainGateway),
			update_flip_supply::UpdateFlipSupply::new(new_total_supply, block_number),
		))
	}
}

impl<E> AllBatch<Ethereum> for EthereumApi<E>
where
	E: EthEnvironmentProvider,
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
					EthereumFetchId::Fetch(contract_address) => {
						debug_assert!(
							asset != assets::eth::Asset::Eth,
							"Eth should not be fetched. It is auto-fetched in the smart contract."
						);
						fetch_only_params.push(EncodableFetchAssetParams {
							contract_address,
							asset: token_address,
						})
					},
					EthereumFetchId::DeployAndFetch(channel_id) => fetch_deploy_params
						.push(EncodableFetchDeployAssetParams { channel_id, asset: token_address }),
					EthereumFetchId::NotRequired => (),
				};
			} else {
				return Err(AllBatchError::Other)
			}
		}
		Ok(Self::AllBatch(EthereumTransactionBuilder::new_unsigned(
			E::replay_protection(EthereumContract::Vault),
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
	E: EthEnvironmentProvider,
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
			E::replay_protection(EthereumContract::Vault),
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
	pub fn replay_protection(&self) -> EthereumReplayProtection {
		map_over_api_variants!(self, call, call.replay_protection())
	}
}

impl<E> ApiCall<Ethereum> for EthereumApi<E> {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		map_over_api_variants!(self, call, call.threshold_signature_payload())
	}

	fn signed(self, threshold_signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
		map_over_api_variants!(self, call, call.signed(threshold_signature).into())
	}

	fn chain_encoded(&self) -> Vec<u8> {
		map_over_api_variants!(self, call, call.chain_encoded())
	}

	fn is_signed(&self) -> bool {
		map_over_api_variants!(self, call, call.is_signed())
	}

	fn transaction_out_id(&self) -> <Ethereum as ChainCrypto>::TransactionOutId {
		map_over_api_variants!(self, call, call.transaction_out_id())
	}
}

pub(super) fn ethabi_param(name: &'static str, param_type: ethabi::ParamType) -> ethabi::Param {
	ethabi::Param { name: name.into(), kind: param_type, internal_type: None }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum EthereumContract {
	StateChainGateway,
	KeyManager,
	Vault,
}

pub type EthereumChainId = u64;
