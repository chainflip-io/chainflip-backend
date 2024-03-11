use crate::{safe_mode, Runtime};
use cf_chains::{arb, eth::Address};
use cf_traits::SafeMode;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_core::H256;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::prelude::*;

pub struct RenameEthereumToEvmThresholdSigner;

impl frame_support::traits::OnRuntimeUpgrade for RenameEthereumToEvmThresholdSigner {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		log::info!("🖋️ Renaming EthereumThresholdSigner to EvmThresholdSigner");
		frame_support::storage::migration::move_pallet(
			b"EthereumThresholdSigner",
			b"EvmThresholdSigner",
		);
		Weight::zero()
	}
}

mod old {
	use super::*;
	use cf_chains::instances::{
		BitcoinCryptoInstance, BitcoinInstance, EthereumInstance, EvmInstance,
		PolkadotCryptoInstance, PolkadotInstance,
	};
	use frame_support::pallet_prelude::*;
	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct RuntimeSafeMode {
		pub emissions: pallet_cf_emissions::PalletSafeMode,
		pub funding: pallet_cf_funding::PalletSafeMode,
		pub swapping: pallet_cf_swapping::PalletSafeMode,
		pub liquidity_provider: pallet_cf_lp::PalletSafeMode,
		pub validator: pallet_cf_validator::PalletSafeMode,
		pub pools: pallet_cf_pools::PalletSafeMode,
		pub reputation: pallet_cf_reputation::PalletSafeMode,
		pub threshold_signature_ethereum:
			pallet_cf_threshold_signature::PalletSafeMode<EvmInstance>,
		pub threshold_signature_bitcoin:
			pallet_cf_threshold_signature::PalletSafeMode<BitcoinCryptoInstance>,
		pub threshold_signature_polkadot:
			pallet_cf_threshold_signature::PalletSafeMode<PolkadotCryptoInstance>,
		pub broadcast_ethereum: pallet_cf_broadcast::PalletSafeMode<EthereumInstance>,
		pub broadcast_bitcoin: pallet_cf_broadcast::PalletSafeMode<BitcoinInstance>,
		pub broadcast_polkadot: pallet_cf_broadcast::PalletSafeMode<PolkadotInstance>,
		pub witnesser: pallet_cf_witnesser::PalletSafeMode<safe_mode::WitnesserCallPermission>,
	}
}
pub struct ArbitrumIntegration;

impl OnRuntimeUpgrade for ArbitrumIntegration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		use cf_chains::{assets::arb::Asset::ArbUsdc, instances::ArbitrumInstance};
		use frame_support::assert_ok;
		use frame_system::pallet_prelude::BlockNumberFor;
		use sp_runtime::traits::Zero;
		use sp_std::str::FromStr;

		assert_ok!(pallet_cf_environment::RuntimeSafeMode::<Runtime>::translate(
			|maybe_old: Option<old::RuntimeSafeMode>| {
				maybe_old.map(|old| {
					safe_mode::RuntimeSafeMode {
							emissions: old.emissions,
							funding: old.funding,
							swapping: old.swapping,
							liquidity_provider: old.liquidity_provider,
							validator: old.validator,
							pools: old.pools,
							reputation: old.reputation,
							threshold_signature_evm: old.threshold_signature_ethereum,
							threshold_signature_bitcoin: old.threshold_signature_bitcoin,
							threshold_signature_polkadot: old.threshold_signature_polkadot,
							broadcast_ethereum: old.broadcast_ethereum,
							broadcast_bitcoin: old.broadcast_bitcoin,
							broadcast_polkadot: old.broadcast_polkadot,
							broadcast_arbitrum: <pallet_cf_broadcast::PalletSafeMode<
								ArbitrumInstance,
							> as SafeMode>::CODE_GREEN,
							witnesser: old.witnesser,
						}
				})
			},
		));

		let genesis_hash =
			frame_system::BlockHash::<Runtime>::get(BlockNumberFor::<Runtime>::zero());

		let (
				key_manager_address,
				vault_address,
				address_checker_address,
				chain_id,
				usdc_address,
			): (Address, Address, Address, u64, Address) = if genesis_hash ==
				// BERGHAIN MAINNET
				H256::from_str(
					"0x8b8c140b0af9db70686583e3f6bf2a59052bfe9584b97d20c45068281e976eb9",
				)
				.unwrap()
			{
				(
					[0u8; 20].into(),
					[0u8; 20].into(),
					[0u8; 20].into(),
					arb::CHAIN_ID_MAINNET,
					[0u8; 20].into(),
				)
			} else if genesis_hash ==
				// PERSEVERANCE
				H256::from_str(
					"0x46c8ca427e31ba73cbd1ad60500d4a7d173b1c80c9fb1afb76661d614f9c5cd7",
				)
				.unwrap()
			{
				(
					[1u8; 20].into(),
					[1u8; 20].into(),
					[1u8; 20].into(),
					arb::CHAIN_ID_GOERLI,
					[1u8; 20].into(),
				)
			} else if genesis_hash ==
				// SISYPHOS
				H256::from_str(
					"0xbeb780f634621c64012483ebbf39927eb236b63902e9a249a76af8ba4cf8a474",
				)
				.unwrap()
			{
				(
					[2u8; 20].into(),
					[2u8; 20].into(),
					[2u8; 20].into(),
					arb::CHAIN_ID_GOERLI,
					[2u8; 20].into(),
				)
			} else {
				// Assume testnet
				(
					hex_literal::hex!("8e1308925a26cb5cF400afb402d67B3523473379").into(),
					hex_literal::hex!("Ce5303b8e8BFCa9d1857976F300fb29928522c6F").into(),
					hex_literal::hex!("84401CD7AbBeBB22ACb7aF2beCfd9bE56C30bcf1").into(),
					412346,
					hex_literal::hex!("1D55838a9EC169488D360783D65e6CD985007b72").into(),
				)
			};

		pallet_cf_environment::ArbitrumKeyManagerAddress::<Runtime>::put(key_manager_address);
		pallet_cf_environment::ArbitrumVaultAddress::<Runtime>::put(vault_address);
		pallet_cf_environment::ArbitrumAddressCheckerAddress::<Runtime>::put(
			address_checker_address,
		);
		pallet_cf_environment::ArbitrumChainId::<Runtime>::put(chain_id);
		pallet_cf_environment::ArbitrumSupportedAssets::<Runtime>::insert(ArbUsdc, usdc_address);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
