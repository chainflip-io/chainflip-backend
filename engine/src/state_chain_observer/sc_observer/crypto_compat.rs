// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use cf_chains::{
	btc::BitcoinCrypto,
	dot::{PolkadotCrypto, PolkadotPublicKey},
	evm::EvmCrypto,
	sol::{SolAddress, SolanaCrypto},
	ChainCrypto,
};
use multisig::{
	bitcoin::BtcSigning, ed25519::SolSigning, eth::EthSigning, polkadot::PolkadotSigning,
	ChainSigning, CryptoScheme,
};
use state_chain_runtime::{BitcoinInstance, EvmInstance, PolkadotCryptoInstance, SolanaInstance};

/// Compatibility layer for converting between public keys generated using the [CryptoScheme] types
/// and the on-chain representation as defined by [ChainCrypto].
pub trait CryptoCompat<S: ChainSigning<ChainCrypto = C>, C: ChainCrypto> {
	fn pubkey_to_aggkey(
		pubkey: <<S as ChainSigning>::CryptoScheme as CryptoScheme>::PublicKey,
	) -> C::AggKey;
}

impl CryptoCompat<EthSigning, EvmCrypto> for EvmInstance {
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

impl CryptoCompat<PolkadotSigning, PolkadotCrypto> for PolkadotCryptoInstance {
	fn pubkey_to_aggkey(
		pubkey: <<PolkadotSigning as ChainSigning>::CryptoScheme as CryptoScheme>::PublicKey,
	) -> <PolkadotCrypto as ChainCrypto>::AggKey {
		PolkadotPublicKey::from_aliased(pubkey.to_bytes())
	}
}

impl CryptoCompat<SolSigning, SolanaCrypto> for SolanaInstance {
	fn pubkey_to_aggkey(
		pubkey: <<SolSigning as ChainSigning>::CryptoScheme as CryptoScheme>::PublicKey,
	) -> <SolanaCrypto as ChainCrypto>::AggKey {
		SolAddress(pubkey.to_bytes())
	}
}
