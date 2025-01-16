use anyhow::bail;
use cf_chains::{
	address::AddressString, Chain, ChainCrypto, ChannelRefundParametersGeneric, ForeignChain,
};
use cf_primitives::AffiliateShortId;
pub use cf_primitives::{AccountRole, Affiliates, Asset, BasisPoints, ChannelId, SemVer};
use cf_utilities::rpc::NumberOrHex;
use frame_support::{Deserialize, Serialize};
use sp_core::{H256, U256};
use std::fmt;

pub type RefundParameters = ChannelRefundParametersGeneric<AddressString>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SwapDepositAddress {
	pub address: AddressString,
	pub issued_block: state_chain_runtime::BlockNumber,
	pub channel_id: ChannelId,
	pub source_chain_expiry_block: NumberOrHex,
	pub channel_opening_fee: U256,
	pub refund_parameters: Option<RefundParameters>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WithdrawFeesDetail {
	pub tx_hash: H256,
	pub egress_id: (ForeignChain, u64),
	pub egress_amount: U256,
	pub egress_fee: U256,
	pub destination_address: AddressString,
}

impl fmt::Display for WithdrawFeesDetail {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(
			f,
			"\
			Tx hash: {:?}\n\
			Egress id: {:?}\n\
			Egress amount: {}\n\
			Egress fee: {}\n\
			Destination address: {}\n\
			",
			self.tx_hash,
			self.egress_id,
			self.egress_amount,
			self.egress_fee,
			self.destination_address,
		)
	}
}

pub type TransactionInIdFor<C> = <<C as Chain>::ChainCrypto as ChainCrypto>::TransactionInId;

#[derive(Serialize, Deserialize)]
pub enum TransactionInId {
	Bitcoin(TransactionInIdFor<cf_chains::Bitcoin>),
	// other variants reserved for other chains.
}

#[derive(Serialize, Deserialize)]
pub enum GetOpenDepositChannelsQuery {
	All,
	Mine,
}

pub fn find_lowest_unused_short_id(
	used_ids: &[AffiliateShortId],
) -> anyhow::Result<AffiliateShortId> {
	let used_id_len = used_ids.len();
	if used_ids.is_empty() {
		Ok(AffiliateShortId::from(0))
	} else if used_id_len > u8::MAX as usize {
		bail!("No unused affiliate short IDs available")
	} else {
		let mut used_ids = used_ids.to_vec();
		used_ids.sort_unstable();
		Ok(AffiliateShortId::from(
			used_ids
				.iter()
				.enumerate()
				.find(|(index, assigned_id)| &AffiliateShortId::from(*index as u8) != *assigned_id)
				.map(|(index, _)| index)
				.unwrap_or(used_id_len) as u8,
		))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_find_lowest_unused_short_id() {
		fn test_lowest(used_ids: &mut Vec<AffiliateShortId>, expected: AffiliateShortId) {
			assert_eq!(find_lowest_unused_short_id(used_ids).unwrap(), expected);
			assert_eq!(
				used_ids.iter().find(|id| *id == &expected),
				None,
				"Should not overwrite existing IDs"
			);
			used_ids.push(expected);
		}

		let mut used_ids = vec![AffiliateShortId::from(1), AffiliateShortId::from(3)];
		test_lowest(&mut used_ids, AffiliateShortId::from(0));
		test_lowest(&mut used_ids, AffiliateShortId::from(2));
		test_lowest(&mut used_ids, AffiliateShortId::from(4));
		test_lowest(&mut used_ids, AffiliateShortId::from(5));
		let mut used_ids: Vec<AffiliateShortId> =
			(0..u8::MAX).map(AffiliateShortId::from).collect();
		test_lowest(&mut used_ids, AffiliateShortId::from(255));
		assert!(find_lowest_unused_short_id(&used_ids).is_err());
	}
}
