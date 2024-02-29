use sol_prim::{Address, Signature, SlotNumber};

use crate::types::Commitment;

pub mod get_fee_for_message;
pub mod get_genesis_hash;
pub mod get_signatures_for_address;
pub mod get_recent_prioritization_fees;
pub mod get_transaction;

#[derive(Debug, Clone)]
pub struct GetFeeForMessage<M> {
	pub message: M,
	pub commitment: Commitment,
}

#[derive(Debug, Clone, Default)]
pub struct GetGenesisHash {}

#[derive(Debug, Clone)]
pub struct GetTransaction {
	pub signature: Signature,
	pub commitment: Commitment,
}

#[derive(Debug, Clone)]
pub struct GetSignaturesForAddress {
	pub address: Address,
	pub before: Option<Signature>,
	pub until: Option<Signature>,
	pub commitment: Commitment,
	pub limit: Option<usize>,
	pub min_context_slot: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct GetSignatureStatuses {
	pub signatures: Vec<Signature>,
	pub search_transaction_history: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GetRecentPrioritizationFees {}