use crate::{
	sol::{
		SolAsset, SolCcmAccounts, SolPubkey, CCM_BYTES_PER_ACCOUNT, MAX_CCM_BYTES_SOL,
		MAX_CCM_BYTES_USDC,
	},
	CcmChannelMetadata, CcmValidityCheck, CcmValidityError,
};
use cf_primitives::{Asset, ForeignChain};
use codec::Decode;

pub struct CcmValidityChecker;

impl CcmValidityCheck for CcmValidityChecker {
	fn is_valid(ccm: &CcmChannelMetadata, egress_asset: Asset) -> Result<(), CcmValidityError> {
		if ForeignChain::from(egress_asset) == ForeignChain::Solana {
			// Check if the cf_parameter can be decoded
			let ccm_accounts = SolCcmAccounts::decode(&mut &ccm.cf_parameters.clone()[..])
				.map_err(|_| CcmValidityError::CannotDecodeCfParameters)?;
			let asset: SolAsset = egress_asset
				.try_into()
				.expect("Only Solana chain's asset will be checked. This conversion must succeed.");

			// Length of CCM = length of message + total no. remaining_accounts * constant;
			let ccm_length =
				ccm.message.len() + ccm_accounts.remaining_accounts.len() * CCM_BYTES_PER_ACCOUNT;
			if ccm_length >
				match asset {
					SolAsset::Sol => MAX_CCM_BYTES_SOL,
					SolAsset::SolUsdc => MAX_CCM_BYTES_USDC,
				} {
				return Err(CcmValidityError::CcmIsTooLong)
			}

			Ok(())
		} else {
			Ok(())
		}
	}
}

/// Checks if the given CCM accounts contains any blacklisted accounts.
pub fn check_ccm_for_blacklisted_accounts(
	ccm_accounts: &SolCcmAccounts,
	blacklisted_accounts: Vec<SolPubkey>,
) -> Result<(), CcmValidityError> {
	blacklisted_accounts.into_iter().try_for_each(|blacklisted_account| {
		(ccm_accounts.cf_receiver.pubkey != blacklisted_account &&
			!ccm_accounts
				.remaining_accounts
				.iter()
				.any(|acc| acc.pubkey == blacklisted_account))
		.then_some(())
		.ok_or(CcmValidityError::CfParametersContainsInvalidAccounts)
	})
}

#[cfg(test)]
mod test {
	use codec::Encode;
	use frame_support::{assert_err, assert_ok};
	use Asset;

	use super::*;
	use crate::sol::{sol_tx_core::sol_test_values, SolCcmAddress, SolPubkey, MAX_CCM_BYTES_SOL};

	#[test]
	fn can_verify_valid_ccm() {
		let ccm = sol_test_values::ccm_parameter().channel_metadata;
		assert_ok!(CcmValidityChecker::is_valid(&ccm, Asset::Sol));
	}

	#[test]
	fn can_check_cf_parameter_decoding() {
		let ccm = CcmChannelMetadata {
			message: vec![0x01, 0x02, 0x03, 0x04, 0x05].try_into().unwrap(),
			gas_budget: 1,
			cf_parameters: vec![0x01, 0x02, 0x03, 0x04, 0x05].try_into().unwrap(),
		};

		assert_err!(
			CcmValidityChecker::is_valid(&ccm, Asset::Sol),
			CcmValidityError::CannotDecodeCfParameters
		);
	}

	#[test]
	fn can_check_for_ccm_length_sol() {
		let ccm = || CcmChannelMetadata {
			message: vec![0x01; MAX_CCM_BYTES_SOL].try_into().unwrap(),
			gas_budget: 0,
			cf_parameters: SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: SolPubkey([0x01; 32]), is_writable: true },
				remaining_accounts: vec![],
			}
			.encode()
			.try_into()
			.unwrap(),
		};
		assert_ok!(CcmValidityChecker::is_valid(&ccm(), Asset::Sol));

		// Length check for Sol
		let mut invalid_ccm = ccm();
		invalid_ccm.message = vec![0x01; MAX_CCM_BYTES_SOL + 1].try_into().unwrap();
		assert_err!(
			CcmValidityChecker::is_valid(&invalid_ccm, Asset::Sol),
			CcmValidityError::CcmIsTooLong
		);

		let mut invalid_ccm = ccm();
		invalid_ccm.cf_parameters = SolCcmAccounts {
			cf_receiver: SolCcmAddress { pubkey: SolPubkey([0x01; 32]), is_writable: true },
			remaining_accounts: vec![SolCcmAddress {
				pubkey: SolPubkey([0x01; 32]),
				is_writable: true,
			}],
		}
		.encode()
		.try_into()
		.unwrap();
		assert_err!(
			CcmValidityChecker::is_valid(&invalid_ccm, Asset::Sol),
			CcmValidityError::CcmIsTooLong
		);
	}

	#[test]
	fn can_check_for_ccm_length_usdc() {
		let ccm = || CcmChannelMetadata {
			message: vec![0x01; MAX_CCM_BYTES_USDC].try_into().unwrap(),
			gas_budget: 0,
			cf_parameters: SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: SolPubkey([0x01; 32]), is_writable: true },
				remaining_accounts: vec![],
			}
			.encode()
			.try_into()
			.unwrap(),
		};
		assert_ok!(CcmValidityChecker::is_valid(&ccm(), Asset::SolUsdc));

		// Length check for SolUsdc
		let mut invalid_ccm = ccm();
		invalid_ccm.message = vec![0x01; MAX_CCM_BYTES_USDC + 1].try_into().unwrap();
		assert_err!(
			CcmValidityChecker::is_valid(&invalid_ccm, Asset::SolUsdc),
			CcmValidityError::CcmIsTooLong
		);

		let mut invalid_ccm = ccm();
		invalid_ccm.cf_parameters = SolCcmAccounts {
			cf_receiver: SolCcmAddress { pubkey: SolPubkey([0x01; 32]), is_writable: true },
			remaining_accounts: vec![SolCcmAddress {
				pubkey: SolPubkey([0x01; 32]),
				is_writable: true,
			}],
		}
		.encode()
		.try_into()
		.unwrap();
		assert_err!(
			CcmValidityChecker::is_valid(&invalid_ccm, Asset::SolUsdc),
			CcmValidityError::CcmIsTooLong
		);
	}

	#[test]
	fn only_check_against_solana_chain() {
		let mut ccm = sol_test_values::ccm_parameter().channel_metadata;

		// Only fails for Solana chain.
		ccm.message = [0x00; MAX_CCM_BYTES_SOL + 1].to_vec().try_into().unwrap();
		assert_err!(CcmValidityChecker::is_valid(&ccm, Asset::Sol), CcmValidityError::CcmIsTooLong);
		ccm.message = [0x00; MAX_CCM_BYTES_USDC + 1].to_vec().try_into().unwrap();
		assert_err!(
			CcmValidityChecker::is_valid(&ccm, Asset::SolUsdc),
			CcmValidityError::CcmIsTooLong
		);

		// Always valid on other chains.
		assert_ok!(CcmValidityChecker::is_valid(&ccm, Asset::Eth),);
		assert_ok!(CcmValidityChecker::is_valid(&ccm, Asset::Btc),);
		assert_ok!(CcmValidityChecker::is_valid(&ccm, Asset::Flip),);
		assert_ok!(CcmValidityChecker::is_valid(&ccm, Asset::Usdt),);
		assert_ok!(CcmValidityChecker::is_valid(&ccm, Asset::Usdc),);
		assert_ok!(CcmValidityChecker::is_valid(&ccm, Asset::ArbUsdc),);
		assert_ok!(CcmValidityChecker::is_valid(&ccm, Asset::ArbEth),);
	}

	#[test]
	fn can_check_for_blacklisted_account() {
		let blacklisted_accounts = || {
			vec![sol_test_values::TOKEN_VAULT_PDA_ACCOUNT.into(), sol_test_values::agg_key().into()]
		};

		// Token vault PDA is blacklisted
		let ccm_accounts = SolCcmAccounts {
			cf_receiver: SolCcmAddress {
				pubkey: sol_test_values::TOKEN_VAULT_PDA_ACCOUNT.into(),
				is_writable: true,
			},
			remaining_accounts: vec![
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x01; 32]), is_writable: false },
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
			],
		};
		assert_err!(
			check_ccm_for_blacklisted_accounts(&ccm_accounts, blacklisted_accounts()),
			CcmValidityError::CfParametersContainsInvalidAccounts
		);

		let ccm_accounts = SolCcmAccounts {
			cf_receiver: SolCcmAddress {
				pubkey: crate::sol::SolPubkey([0x01; 32]),
				is_writable: true,
			},
			remaining_accounts: vec![
				SolCcmAddress {
					pubkey: sol_test_values::TOKEN_VAULT_PDA_ACCOUNT.into(),
					is_writable: false,
				},
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
			],
		};
		assert_err!(
			check_ccm_for_blacklisted_accounts(&ccm_accounts, blacklisted_accounts()),
			CcmValidityError::CfParametersContainsInvalidAccounts
		);

		// Agg key is blacklisted
		let ccm_accounts = SolCcmAccounts {
			cf_receiver: SolCcmAddress {
				pubkey: sol_test_values::agg_key().into(),
				is_writable: true,
			},
			remaining_accounts: vec![
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x01; 32]), is_writable: false },
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
			],
		};
		assert_err!(
			check_ccm_for_blacklisted_accounts(&ccm_accounts, blacklisted_accounts()),
			CcmValidityError::CfParametersContainsInvalidAccounts
		);

		let ccm_accounts = SolCcmAccounts {
			cf_receiver: SolCcmAddress {
				pubkey: crate::sol::SolPubkey([0x01; 32]),
				is_writable: true,
			},
			remaining_accounts: vec![
				SolCcmAddress { pubkey: sol_test_values::agg_key().into(), is_writable: false },
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
			],
		};
		assert_err!(
			check_ccm_for_blacklisted_accounts(&ccm_accounts, blacklisted_accounts()),
			CcmValidityError::CfParametersContainsInvalidAccounts
		);
	}
}
