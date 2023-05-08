use cf_chains::{dot::PolkadotPublicKey, ChainCrypto};
use multisig::{bitcoin::BtcSigning, eth::EthSigning, polkadot::PolkadotSigning, CryptoScheme};
use state_chain_runtime::{BitcoinInstance, EthereumInstance, PolkadotInstance};

/// Compatibility layer for converting between public keys generated using the [CryptoScheme] types
/// and the on-chain representation as defined by [ChainCrypto].
pub trait CryptoCompat<S: CryptoScheme<Chain = C>, C: ChainCrypto> {
	fn pubkey_to_aggkey(pubkey: S::PublicKey) -> C::AggKey;
}

impl CryptoCompat<EthSigning, cf_chains::Ethereum> for EthereumInstance {
	fn pubkey_to_aggkey(
		pubkey: <EthSigning as CryptoScheme>::PublicKey,
	) -> <cf_chains::Ethereum as ChainCrypto>::AggKey {
		pubkey
	}
}

impl CryptoCompat<BtcSigning, cf_chains::Bitcoin> for BitcoinInstance {
	fn pubkey_to_aggkey(
		pubkey: <BtcSigning as CryptoScheme>::PublicKey,
	) -> <cf_chains::Bitcoin as ChainCrypto>::AggKey {
		cf_chains::btc::AggKey { pubkey_x: pubkey.serialize() }
	}
}

impl CryptoCompat<PolkadotSigning, cf_chains::Polkadot> for PolkadotInstance {
	fn pubkey_to_aggkey(
		pubkey: <PolkadotSigning as CryptoScheme>::PublicKey,
	) -> <cf_chains::Polkadot as ChainCrypto>::AggKey {
		PolkadotPublicKey(sp_core::sr25519::Public::from_raw(pubkey.to_bytes()))
	}
}
