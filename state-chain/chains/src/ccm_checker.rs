use crate::{
	sol::{
		api::SolanaEnvironment, SolAsset, SolCcmAccounts, CCM_BYTES_PER_ACCOUNT, MAX_CCM_BYTES_SOL,
		MAX_CCM_BYTES_USDC,
	},
	CcmChannelMetadata, CcmValidityCheck, CcmValidityError,
};
use cf_primitives::{Asset, ForeignChain};
use codec::Decode;
use core::marker::PhantomData;

pub struct CcmValidityChecker<Environment> {
	_phantom: PhantomData<Environment>,
}

impl<Environment: SolanaEnvironment> CcmValidityCheck for CcmValidityChecker<Environment> {
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

			// Check if the parameter accounts are valid
			if let Ok(api_env) = Environment::api_environment() {
				let token_pda = api_env.token_vault_pda_account.into();
				if ccm_accounts.cf_receiver.pubkey == token_pda ||
					ccm_accounts.remaining_accounts.iter().any(|acc| acc.pubkey == token_pda)
				{
					return Err(CcmValidityError::CfParametersContainsInvalidAccounts)
				}
			};
			if let Ok(agg_key) = Environment::current_agg_key() {
				let agg_key = agg_key.into();
				if ccm_accounts.cf_receiver.pubkey == agg_key ||
					ccm_accounts.remaining_accounts.iter().any(|acc| acc.pubkey == agg_key)
				{
					return Err(CcmValidityError::CfParametersContainsInvalidAccounts)
				}
			};

			Ok(())
		} else {
			Ok(())
		}
	}
}

#[cfg(test)]
mod test {
	use codec::Encode;
	use frame_support::{assert_err, assert_ok};
	use Asset;

	use super::*;
	use crate::{
		sol::{
			api::{
				AllNonceAccounts, ApiEnvironment, ComputePrice, CurrentAggKey, DurableNonce,
				DurableNonceAndAccount,
			},
			signing_key::SolSigningKey,
			sol_tx_core::{signer::Signer, sol_test_values},
			SolAddress, SolAmount, SolApiEnvironment, SolCcmAddress, SolPubkey, MAX_CCM_BYTES_SOL,
		},
		ChainEnvironment,
	};

	pub struct MockEnv;

	impl ChainEnvironment<ApiEnvironment, SolApiEnvironment> for MockEnv {
		fn lookup(_s: ApiEnvironment) -> Option<SolApiEnvironment> {
			Some(SolApiEnvironment {
				vault_program: sol_test_values::VAULT_PROGRAM,
				vault_program_data_account: sol_test_values::VAULT_PROGRAM_DATA_ACCOUNT,
				token_vault_pda_account: sol_test_values::TOKEN_VAULT_PDA_ACCOUNT,
				usdc_token_mint_pubkey: sol_test_values::USDC_TOKEN_MINT_PUB_KEY,
				usdc_token_vault_ata: sol_test_values::USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
			})
		}
	}

	impl ChainEnvironment<CurrentAggKey, SolAddress> for MockEnv {
		fn lookup(_s: CurrentAggKey) -> Option<SolAddress> {
			Some(
				SolSigningKey::from_bytes(&sol_test_values::RAW_KEYPAIR)
					.expect("Key pair generation must succeed")
					.pubkey()
					.into(),
			)
		}
	}

	impl ChainEnvironment<ComputePrice, SolAmount> for MockEnv {
		fn lookup(_s: ComputePrice) -> Option<u64> {
			None
		}
	}

	impl ChainEnvironment<DurableNonce, DurableNonceAndAccount> for MockEnv {
		fn lookup(_s: DurableNonce) -> Option<DurableNonceAndAccount> {
			None
		}
	}

	impl ChainEnvironment<AllNonceAccounts, Vec<DurableNonceAndAccount>> for MockEnv {
		fn lookup(_s: AllNonceAccounts) -> Option<Vec<DurableNonceAndAccount>> {
			None
		}
	}

	impl crate::sol::api::RecoverDurableNonce for MockEnv {}

	impl SolanaEnvironment for MockEnv {}

	#[test]
	fn can_verify_valid_ccm() {
		let ccm = sol_test_values::ccm_parameter().channel_metadata;
		assert_ok!(CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::Sol));
	}

	#[test]
	fn can_check_cf_parameter_decoding() {
		let ccm = CcmChannelMetadata {
			message: vec![0x01, 0x02, 0x03, 0x04, 0x05].try_into().unwrap(),
			gas_budget: 1,
			cf_parameters: vec![0x01, 0x02, 0x03, 0x04, 0x05].try_into().unwrap(),
		};

		assert_err!(
			CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::Sol),
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
		assert_ok!(CcmValidityChecker::<MockEnv>::is_valid(&ccm(), Asset::Sol));

		// Length check for Sol
		let mut invalid_ccm = ccm();
		invalid_ccm.message = vec![0x01; MAX_CCM_BYTES_SOL + 1].try_into().unwrap();
		assert_err!(
			CcmValidityChecker::<MockEnv>::is_valid(&invalid_ccm, Asset::Sol),
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
			CcmValidityChecker::<MockEnv>::is_valid(&invalid_ccm, Asset::Sol),
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
		assert_ok!(CcmValidityChecker::<MockEnv>::is_valid(&ccm(), Asset::SolUsdc));

		// Length check for SolUsdc
		let mut invalid_ccm = ccm();
		invalid_ccm.message = vec![0x01; MAX_CCM_BYTES_USDC + 1].try_into().unwrap();
		assert_err!(
			CcmValidityChecker::<MockEnv>::is_valid(&invalid_ccm, Asset::SolUsdc),
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
			CcmValidityChecker::<MockEnv>::is_valid(&invalid_ccm, Asset::SolUsdc),
			CcmValidityError::CcmIsTooLong
		);
	}

	#[test]
	fn can_for_blacklisted_account() {
		let mut ccm = sol_test_values::ccm_parameter().channel_metadata;

		// Token vault PDA is blacklisted
		ccm.cf_parameters = SolCcmAccounts {
			cf_receiver: SolCcmAddress {
				pubkey: sol_test_values::TOKEN_VAULT_PDA_ACCOUNT.into(),
				is_writable: true,
			},
			remaining_accounts: vec![
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x01; 32]), is_writable: false },
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
			],
		}
		.encode()
		.try_into()
		.unwrap();
		assert_err!(
			CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::Sol),
			CcmValidityError::CfParametersContainsInvalidAccounts
		);

		ccm.cf_parameters = SolCcmAccounts {
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
		}
		.encode()
		.try_into()
		.unwrap();
		assert_err!(
			CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::Sol),
			CcmValidityError::CfParametersContainsInvalidAccounts
		);

		// Agg key is blacklisted
		let agg_key = MockEnv::current_agg_key().unwrap();
		ccm.cf_parameters = SolCcmAccounts {
			cf_receiver: SolCcmAddress { pubkey: agg_key.into(), is_writable: true },
			remaining_accounts: vec![
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x01; 32]), is_writable: false },
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
			],
		}
		.encode()
		.try_into()
		.unwrap();
		assert_err!(
			CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::Sol),
			CcmValidityError::CfParametersContainsInvalidAccounts
		);

		ccm.cf_parameters = SolCcmAccounts {
			cf_receiver: SolCcmAddress {
				pubkey: crate::sol::SolPubkey([0x01; 32]),
				is_writable: true,
			},
			remaining_accounts: vec![
				SolCcmAddress { pubkey: agg_key.into(), is_writable: false },
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
			],
		}
		.encode()
		.try_into()
		.unwrap();
		assert_err!(
			CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::Sol),
			CcmValidityError::CfParametersContainsInvalidAccounts
		);
	}

	#[test]
	fn only_check_against_solana_chain() {
		let mut ccm = sol_test_values::ccm_parameter().channel_metadata;

		// Only fails for Solana chain.
		ccm.message = vec![0x00; MAX_CCM_BYTES_SOL].try_into().unwrap();
		assert_err!(
			CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::Sol),
			CcmValidityError::CcmIsTooLong
		);
		ccm.message = vec![0x00; MAX_CCM_BYTES_USDC].try_into().unwrap();
		assert_err!(
			CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::SolUsdc),
			CcmValidityError::CcmIsTooLong
		);

		// Always valid on other chains.
		assert_ok!(CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::Eth),);
		assert_ok!(CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::Btc),);
		assert_ok!(CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::Flip),);
		assert_ok!(CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::Usdt),);
		assert_ok!(CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::Usdc),);
		assert_ok!(CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::ArbUsdc),);
		assert_ok!(CcmValidityChecker::<MockEnv>::is_valid(&ccm, Asset::ArbEth),);
	}
}
