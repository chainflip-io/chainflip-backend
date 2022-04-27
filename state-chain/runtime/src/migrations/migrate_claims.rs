use crate::AccountId;
use cf_chains::eth::{api::register_claim::RegisterClaim, Address, SigData, Uint, H256};
use codec::{Decode, Encode};
use frame_support::{
	storage::migration, traits::OnRuntimeUpgrade, weights::RuntimeDbWeight, Blake2_128Concat,
	StorageHasher,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
struct OldSigData {
	pub msg_hash: H256,
	pub sig: Uint,
	pub nonce: Uint,
	pub k_times_g_address: Address,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
struct OldRegisterClaim {
	sig_data: OldSigData,
	pub node_id: [u8; 32],
	pub amount: Uint,
	pub address: Address,
	pub expiry: Uint,
}

impl From<OldSigData> for SigData {
	fn from(old: OldSigData) -> Self {
		Self::from_legacy(old.msg_hash, old.sig, old.nonce, old.k_times_g_address)
	}
}

impl From<OldRegisterClaim> for RegisterClaim {
	fn from(old: OldRegisterClaim) -> Self {
		Self {
			sig_data: old.sig_data.into(),
			node_id: old.node_id,
			amount: old.amount,
			address: old.address,
			expiry: old.expiry,
		}
	}
}

/// A migration that transcodes pending claims to the new SigData format.
pub struct Migration;

const PALLET_NAME: &[u8] = b"Staking";
const STORAGE_NAME: &[u8] = b"PendingClaims";

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		for (id, old_claim) in migration::storage_key_iter::<
			AccountId,
			OldRegisterClaim,
			Blake2_128Concat,
		>(PALLET_NAME, STORAGE_NAME)
		.drain()
		{
			migration::put_storage_value::<RegisterClaim>(
				PALLET_NAME,
				STORAGE_NAME,
				&id.using_encoded(|bytes| <Blake2_128Concat as StorageHasher>::hash(bytes)),
				old_claim.into(),
			)
		}

		RuntimeDbWeight::default().reads_writes(0, 1)
	}
}
