pub mod x_swap_native;
pub mod x_swap_token;

/// Some test values and utility functions used within the Vault swap call module.
#[cfg(test)]
pub mod test_utils {
	use crate::{cf_parameters::*, ChannelRefundParameters, ForeignChainAddress};
	use cf_primitives::{
		AccountId, AffiliateAndFee, AffiliateShortId, Beneficiary, DcaParameters, MAX_AFFILIATES,
	};
	use frame_support::pallet_prelude::ConstU32;
	use sp_runtime::BoundedVec;

	pub fn refund_address() -> ForeignChainAddress {
		ForeignChainAddress::Eth([0xF0; 20].into())
	}
	pub fn dca_parameter() -> DcaParameters {
		DcaParameters { number_of_chunks: 10u32, chunk_interval: 5u32 }
	}
	pub fn affiliate_fees() -> BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>> {
		vec![AffiliateAndFee { affiliate: AffiliateShortId(1u8), fee: 10u8 }]
			.try_into()
			.unwrap()
	}
	pub fn broker_fee() -> Beneficiary<AccountId> {
		Beneficiary { account: AccountId::from([0xF2; 32]), bps: 1u16 }
	}

	pub const BOOST_FEE: u8 = 100u8;
	pub const BROKER_FEE: u8 = 150u8;

	pub fn dummy_cf_parameter_no_ccm() -> VersionedCfParameters {
		VersionedCfParameters::V0(CfParameters {
			ccm_additional_data: (),
			vault_swap_parameters: VaultSwapParameters {
				refund_params: ChannelRefundParameters {
					retry_duration: 1u32,
					refund_address: refund_address(),
					min_price: Default::default(),
				},
				dca_params: Some(dca_parameter()),
				boost_fee: BOOST_FEE,
				broker_fee: broker_fee(),
				affiliate_fees: affiliate_fees(),
			},
		})
	}
}
