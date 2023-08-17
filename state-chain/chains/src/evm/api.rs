pub mod common;
pub mod tokenizable;

use crate::{
	eth::{api::all_batch, Address as EvmAddress, EthereumFetchId},
	evm::{api::common::EncodableFetchAssetParams, SchnorrVerificationComponents},
	*,
};
use ethabi::{ParamType, Token, Uint};
use frame_support::sp_runtime::traits::{Hash, Keccak256};

pub use tokenizable::Tokenizable;

use self::common::EncodableFetchDeployAssetParams;
use crate::evm::api::common::EncodableTransferAssetParams;

use super::EthereumChainId;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Default)]
pub struct EvmReplayProtection {
	pub nonce: u64,
	pub chain_id: EthereumChainId,
	pub key_manager_address: EvmAddress,
	pub contract_address: EvmAddress,
}

impl Tokenizable for EvmReplayProtection {
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

pub trait SCGatewayProvider {
	fn state_chain_gateway_address() -> EvmAddress;
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
	pub sig: Uint,
	/// The nonce value for the AggKey. Each Signature over an AggKey should have a unique
	/// nonce to prevent replay attacks.
	pub nonce: Uint,
	/// The address value derived from the random nonce value `k`. Also known as
	/// `nonceTimesGeneratorAddress`.
	///
	/// Note this is unrelated to the `nonce` above. The nonce in the context of
	/// `nonceTimesGeneratorAddress` is a generated as part of each signing round (ie. as part
	/// of the Schnorr signature) to prevent certain classes of cryptographic attacks.
	pub k_times_g_address: EvmAddress,
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
	pub sig_data: Option<SigData>,
	pub replay_protection: EvmReplayProtection,
	pub call: C,
}

impl<C: EthereumCall> EthereumTransactionBuilder<C> {
	pub fn new_unsigned(replay_protection: EvmReplayProtection, call: C) -> Self {
		Self { replay_protection, call, sig_data: None }
	}

	pub fn replay_protection(&self) -> EvmReplayProtection {
		self.replay_protection
	}

	pub fn chain_id(&self) -> EthereumChainId {
		self.replay_protection.chain_id
	}
}

pub fn evm_all_batch_builder<
	C: Chain<DepositFetchId = EthereumFetchId, ChainAccount = EvmAddress, ChainAmount = u128>,
	F: Fn(<C as Chain>::ChainAsset) -> Option<EvmAddress>,
>(
	fetch_params: Vec<FetchAssetParams<C>>,
	transfer_params: Vec<TransferAssetParams<C>>,
	token_address_fn: F,
	replay_protection: EvmReplayProtection,
) -> Result<EthereumTransactionBuilder<all_batch::AllBatch>, AllBatchError> {
	let mut fetch_only_params = vec![];
	let mut fetch_deploy_params = vec![];
	for FetchAssetParams { deposit_fetch_id, asset } in fetch_params {
		if let Some(token_address) = token_address_fn(asset) {
			match deposit_fetch_id {
				EthereumFetchId::Fetch(contract_address) => fetch_only_params
					.push(EncodableFetchAssetParams { contract_address, asset: token_address }),
				EthereumFetchId::DeployAndFetch(channel_id) => fetch_deploy_params
					.push(EncodableFetchDeployAssetParams { channel_id, asset: token_address }),
				EthereumFetchId::NotRequired => (),
			};
		} else {
			return Err(AllBatchError::Other)
		}
	}
	Ok(EthereumTransactionBuilder::new_unsigned(
		replay_protection,
		all_batch::AllBatch::new(
			fetch_deploy_params,
			fetch_only_params,
			transfer_params
				.into_iter()
				.map(|TransferAssetParams { asset, to, amount }| {
					token_address_fn(asset)
						.map(|address| EncodableTransferAssetParams { to, amount, asset: address })
						.ok_or(AllBatchError::Other)
				})
				.collect::<Result<Vec<_>, _>>()?,
		),
	))
}

pub(super) fn ethabi_param(name: &'static str, param_type: ethabi::ParamType) -> ethabi::Param {
	ethabi::Param { name: name.into(), kind: param_type, internal_type: None }
}
