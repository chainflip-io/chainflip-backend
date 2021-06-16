use codec::Encode;
use core::time::Duration;
use ethereum_types::H256;
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::marker::PhantomData;

use super::{ClaimDetailsFor, Config};
use ethereum_types::{self, Address, U256};
use rlp::Encodable;
use sp_core::hashing::keccak_256;

const CLAIM_FN_SIG: &'static str = "registerClaim((uint256,uint256,uint256),bytes32,uint256,address,uint48)";

/// Converts an ethereum function signature
fn selector_from_fn_sig(fn_sig: &str) -> [u8; 4] {
	let mut buffer = [0u8; 4];
	let hash = keccak_256(fn_sig.as_bytes());
	buffer.copy_from_slice(&hash[..4]);
	buffer
}

pub(crate) struct ClaimRequestPayload<T> {
	selector: [u8; 4],
	sig_data: SigData,
	node_id: H256,
	amount: U256,
	staker: Address,
	expiry_time: ExpiryU48,
	_phantom: PhantomData<T>,
}

impl<T: Config> ClaimRequestPayload<T> {
	pub fn to_encoded(&self) -> Vec<u8> {
		rlp::encode(self).to_vec()
	}
}

impl<T: Config> Encodable for ClaimRequestPayload<T> {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		Encodable::rlp_append(&FixedSizeArrayWrapper(self.selector), s);
		Encodable::rlp_append(&self.sig_data, s);
		Encodable::rlp_append(&self.node_id, s);
		Encodable::rlp_append(&self.amount, s);
		Encodable::rlp_append(&self.staker, s);
		Encodable::rlp_append(&self.expiry_time, s);
	}
}

struct SigData(U256, U256, U256);

impl SigData {
	fn null_with_nonce(nonce: u64) -> SigData {
		Self(U256::zero(), U256::zero(), U256::from(nonce))
	}
}

impl Encodable for SigData {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		Encodable::rlp_append(&self.0, s);
		Encodable::rlp_append(&self.1, s);
		Encodable::rlp_append(&self.2, s);
	}
}

struct FixedSizeArrayWrapper<const S: usize>([u8; S]);

impl<const S: usize> Encodable for FixedSizeArrayWrapper<S> {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		Encodable::rlp_append(&&self.0[..], s)
	}
}

/// Stake expiry is measured in seconds, encoded as a uint48 in ethereum.
struct ExpiryU48(u64);

impl Encodable for ExpiryU48 {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		Encodable::rlp_append(&&self.0.to_be_bytes()[2..], s)
	}
}

impl From<Duration> for ExpiryU48 {
	fn from(d: Duration) -> Self {
		Self(d.as_secs())
	}
}

impl<T: Config> From<(&T::AccountId, &ClaimDetailsFor<T>)> for ClaimRequestPayload<T> {
	fn from(details: (&T::AccountId, &ClaimDetailsFor<T>)) -> Self {
		let (account_id, claim_details) = details;
		let nonce: u64 = claim_details.nonce.unique_saturated_into();
		let amount: u128 = claim_details.amount.unique_saturated_into();

		Self {
			selector: selector_from_fn_sig(CLAIM_FN_SIG),
			sig_data: SigData::null_with_nonce(nonce),
			node_id: account_id_to_node_id::<T>(account_id),
			amount: amount.into(),
			staker: claim_details.address.into(),
			expiry_time: claim_details.expiry.into(),
			_phantom: Default::default(),
		}
	}
}

fn account_id_to_node_id<T: Config>(account_id: &T::AccountId) -> H256{
	let account_bytes = account_id.using_encoded(|bytes| bytes.to_vec());
	let mut node_id = [0u8; 32];

	match account_bytes.len() {
		len if len < 32 => {
			node_id[(32 - len)..].copy_from_slice(&account_bytes[..]);
		}
		len if len > 32 => {
			node_id.copy_from_slice(&account_bytes[..32]);
		}
		_ => {
			node_id.copy_from_slice(&account_bytes[..]);
		}
	};

	H256::from(node_id)
}
