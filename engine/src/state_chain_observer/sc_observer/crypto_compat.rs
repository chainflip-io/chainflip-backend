use cf_chains::{
	btc::BitcoinCrypto,
	dot::{PolkadotCrypto, PolkadotPublicKey},
	evm::EvmCrypto,
	ChainCrypto,
};
use multisig::{
	bitcoin::BtcSigning, eth::EthSigning, polkadot::PolkadotSigning, ChainSigning, CryptoScheme,
};
use state_chain_runtime::{BitcoinInstance, EthereumInstance, PolkadotInstance};

/// Compatibility layer for converting between public keys generated using the [CryptoScheme] types
/// and the on-chain representation as defined by [ChainCrypto].
pub trait CryptoCompat<S: ChainSigning<ChainCrypto = C>, C: ChainCrypto> {
	fn pubkey_to_aggkey(
		pubkey: <<S as ChainSigning>::CryptoScheme as CryptoScheme>::PublicKey,
	) -> C::AggKey;
}

impl CryptoCompat<EthSigning, EvmCrypto> for EthereumInstance {
	fn pubkey_to_aggkey(
		pubkey: <<EthSigning as ChainSigning>::CryptoScheme as CryptoScheme>::PublicKey,
	) -> <EvmCrypto as ChainCrypto>::AggKey {
		pubkey
	}
}

impl CryptoCompat<BtcSigning, BitcoinCrypto> for BitcoinInstance {
	fn pubkey_to_aggkey(
		pubkey: <<BtcSigning as ChainSigning>::CryptoScheme as CryptoScheme>::PublicKey,
	) -> <BitcoinCrypto as ChainCrypto>::AggKey {
		cf_chains::btc::AggKey { previous: None, current: pubkey.serialize() }
	}
}

impl CryptoCompat<PolkadotSigning, PolkadotCrypto> for PolkadotInstance {
	fn pubkey_to_aggkey(
		pubkey: <<PolkadotSigning as ChainSigning>::CryptoScheme as CryptoScheme>::PublicKey,
	) -> <PolkadotCrypto as ChainCrypto>::AggKey {
		PolkadotPublicKey::from_aliased(pubkey.to_bytes())
	}
}
