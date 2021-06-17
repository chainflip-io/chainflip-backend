use core::time::Duration;
use sp_runtime::traits::{Hash, Keccak256, UniqueSaturatedInto};
use sp_std::prelude::*;
use sp_std::marker::PhantomData;

use super::{ClaimDetailsFor, Config};
use ethereum_types::{self, Address, U256};

const CLAIM_FN_SIG: &'static str =
	"registerClaim((uint256,uint256,uint256),bytes32,uint256,address,uint48)";

/// Takes claim request details for an account and encodes the payload that needs to be signed for a claim request.
pub(crate) fn encode_claim_request<T: Config>(
	account_id: &T::AccountId,
	claim_details: &ClaimDetailsFor<T>,
) -> Vec<u8> {
	ClaimRequestPayload::<T>::from((account_id, claim_details)).abi_encode()
}

/// A very simple trait for encoding to an ethereum ABI-compatible byte representation.
trait EthAbiEncode {
	/// The number of bytes returned, once encoded.
	///
	/// This is used to initialise the byte buffer to avoid unnecessary allocations.
	const ENCODED_SIZE: usize;

	/// Encode the contents of `self` onto the end of the provided buffer.
	fn encode_to(&self, buffer: &mut Vec<u8>);

	fn abi_encode(&self) -> Vec<u8> {
		let mut bytes = Vec::with_capacity(Self::ENCODED_SIZE);
		self.encode_to(&mut bytes);

		bytes
	}
}

/// The payload to be signed for the `registerClaim` StakeManager contract call.
struct ClaimRequestPayload<T> {
	selector: FunctionSelector,
	sig_data: SigData,
	node_id: Bytes32,
	amount: U256,
	staker: Address,
	expiry_time: ExpirySecs,
	_phantom: PhantomData<T>,
}

impl<T: Config> EthAbiEncode for ClaimRequestPayload<T> {
	const ENCODED_SIZE: usize = FunctionSelector::ENCODED_SIZE
		+ SigData::ENCODED_SIZE
		+ Bytes32::ENCODED_SIZE
		+ U256::ENCODED_SIZE
		+ Address::ENCODED_SIZE
		+ ExpirySecs::ENCODED_SIZE;

	fn encode_to(&self, buffer: &mut Vec<u8>) {
		self.selector.encode_to(buffer);
		self.sig_data.encode_to(buffer);
		self.node_id.encode_to(buffer);
		self.amount.encode_to(buffer);
		self.staker.encode_to(buffer);
		self.expiry_time.encode_to(buffer);
	}
}

/// Analog for the SigData struct defined in solidity.
struct SigData(U256, U256, U256);

impl SigData {
	fn null_with_nonce(nonce: u64) -> SigData {
		Self(U256::zero(), U256::zero(), U256::from(nonce))
	}
}

impl EthAbiEncode for SigData {
	const ENCODED_SIZE: usize = U256::ENCODED_SIZE * 3;

	fn encode_to(&self, buffer: &mut Vec<u8>) {
		self.0.encode_to(buffer);
		self.1.encode_to(buffer);
		self.2.encode_to(buffer);
	}
}

struct Bytes32([u8; 32]);

impl EthAbiEncode for Bytes32 {
	const ENCODED_SIZE: usize = 32;

	fn encode_to(&self, buffer: &mut Vec<u8>) {
		buffer.extend_from_slice(&self.0[..]);
	}
}

impl EthAbiEncode for U256 {
	const ENCODED_SIZE: usize = 32;

	fn encode_to(&self, buffer: &mut Vec<u8>) {
		let mut bytes = [0u8; Self::ENCODED_SIZE];
		self.to_big_endian(&mut bytes[..]);
		buffer.extend_from_slice(&bytes[..]);
	}
}

impl EthAbiEncode for Address {
	const ENCODED_SIZE: usize = 32;

	fn encode_to(&self, buffer: &mut Vec<u8>) {
		const ADDRESS_LEN: usize = 20;
		// For some reason can't use Self::ENCODED_SIZE here:
		const PADDING_SIZE: usize = 32 - ADDRESS_LEN;

		buffer.extend(&[0u8; PADDING_SIZE]);
		buffer.extend(self.as_bytes());
	}
}

/// Wrapper for a 4-byte ethereum function selector.
struct FunctionSelector([u8; 4]);

impl FunctionSelector {
	/// Converts an ethereum function signature to its selector.
	fn from_fn_sig(fn_sig: &str) -> Self {
		let mut buffer = [0u8; 4];
		let hash = Keccak256::hash(fn_sig.as_bytes());
		buffer.copy_from_slice(&hash[..4]);
		FunctionSelector(buffer)
	}
}

impl EthAbiEncode for FunctionSelector {
	const ENCODED_SIZE: usize = 4;

	fn encode_to(&self, buffer: &mut Vec<u8>) {
		buffer.extend_from_slice(&self.0[..])
	}
}

/// Stake expiry is measured in seconds, encoded as a uint48 in ethereum.
///
/// Note that even though the expiry seconds are declared as uint48, they encode to a full 32-byte word in abi terms.
struct ExpirySecs(u64);

impl From<Duration> for ExpirySecs {
	fn from(d: Duration) -> Self {
		Self(d.as_secs())
	}
}

impl EthAbiEncode for ExpirySecs {
	const ENCODED_SIZE: usize = 32;

	fn encode_to(&self, buffer: &mut Vec<u8>) {
		U256::from(self.0).encode_to(buffer)
	}
}

impl<T: Config> From<(&T::AccountId, &ClaimDetailsFor<T>)> for ClaimRequestPayload<T> {
	fn from(details: (&T::AccountId, &ClaimDetailsFor<T>)) -> Self {
		let (account_id, claim_details) = details;
		let nonce: u64 = claim_details.nonce.unique_saturated_into();
		let amount: u128 = claim_details.amount.unique_saturated_into();

		Self {
			selector: FunctionSelector::from_fn_sig(CLAIM_FN_SIG),
			sig_data: SigData::null_with_nonce(nonce),
			node_id: account_id_to_node_id::<T>(account_id),
			amount: amount.into(),
			staker: claim_details.address.into(),
			expiry_time: claim_details.expiry.into(),
			_phantom: Default::default(),
		}
	}
}

/// Converts one of our account ids to the corresponding ethereum bytes32 ethereum abi type. If the AccountId type
/// encodes to more than 32 bytes, it is truncated. If it encode to fewer bytes, they are right-padded with zeros.
fn account_id_to_node_id<T: Config>(account_id: &T::AccountId) -> Bytes32 {
	// Abuse parity SCALE to get to the raw bytes.
	let account_bytes = codec::Encode::using_encoded(account_id, |bytes| bytes.to_vec());

	let mut node_id = [0u8; 32];
	match account_bytes.len() {
		len if len < 32 => {
			node_id[..len].copy_from_slice(&account_bytes[..]);
		}
		len if len > 32 => {
			node_id.copy_from_slice(&account_bytes[..32]);
		}
		_ => {
			node_id.copy_from_slice(&account_bytes[..]);
		}
	};

	Bytes32(node_id)
}
