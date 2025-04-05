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

use crate::{
	btc::BitcoinCrypto,
	dot::PolkadotCrypto,
	evm::EvmCrypto,
	none::{NoneChain, NoneChainCrypto},
	sol::SolanaCrypto,
	AnyChain, Arbitrum, Bitcoin, Ethereum, Polkadot, Solana,
};
use frame_support::instances::*;

pub type CryptoInstanceFor<C> = <C as ChainCryptoInstanceAlias>::Instance;
pub type ChainInstanceFor<C> = <C as ChainInstanceAlias>::Instance;

/// Allows a type to be used as an alias for a pallet `Instance`.
pub trait PalletInstanceAlias {
	type Instance: Send + Sync + 'static;
	const TYPE_INFO_SUFFIX: &'static str;
}

/// Allows a type to be used as an alias for a [Chain] `Instance`.
///
/// Every [Chain] must have a corresponding [ChainCrypto] and therefore every ChainInstanceAlias
/// implies a [ChainCryptoInstanceAlias].
pub trait ChainInstanceAlias: ChainCryptoInstanceAlias + PalletInstanceAlias {
	type Instance: Send + Sync + 'static;
	const TYPE_INFO_SUFFIX: &'static str;
}

/// Allows a type to be used as an alias for a [ChainCrypto] `Instance`.
pub trait ChainCryptoInstanceAlias: PalletInstanceAlias {
	type Instance: Send + Sync + 'static;
	const TYPE_INFO_SUFFIX: &'static str;
}

/// Declare pallet instance aliases.
///
/// Syntax: `decl_instance_aliases!(<chain_or_crypto> => <type_alias>, <instance>);`
///
/// # Example
///
/// ```ignore
/// decl_instance_aliases!(
///     Ethereum => EthereumInstance, Instance1,
/// );
/// ```
///
/// This would result in the following expanded code:
///
/// ```ignore
/// impl PalletInstanceAlias for Ethereum {
///     type Instance = Instance1;
/// }
/// // Equivalent to `pub type EthereumInstance = Instance1`
/// pub type EthereumInstance = <Ethereum as PalletInstanceAlias>::Instance;
/// ```
#[macro_export]
macro_rules! decl_instance_aliases {
	( $( $chain_or_crypto:ty => $name:ident, $instance:ty $(,)? )+ ) => {
		$(
			impl $crate::instances::PalletInstanceAlias for $chain_or_crypto {
				type Instance = $instance;
				const TYPE_INFO_SUFFIX: &'static str = ::core::stringify!($chain_or_crypto);
			}
			pub type $name = <$chain_or_crypto as $crate::instances::PalletInstanceAlias>::Instance;
		)+
	};
}

/// Implement instance alias traits for the given chain and crypto types.
///
/// Syntax: `impl_instance_alias_traits!(<crypto> => { <chain>,+ },+);`
///
/// # Example
///
/// ```ignore
/// impl_instance_alias_traits!(
///     EvmCrypto => { Ethereum },
/// );
/// ```
///
/// This would result in the following expanded code:
///
/// ```ignore
/// // Implements the alias for the crypto type.
/// impl ChainCryptoInstanceAlias for EvmCrypto {
///     type Instance = <EvmCrypto as PalletInstanceAlias>::Instance;
/// }
///
/// // Implements the alias for the chain type.
/// impl ChainInstanceAlias for Ethereum {
///     type Instance = <Ethereum as PalletInstanceAlias>::Instance;
/// }
///
/// // The ChainCryptoInstanceAlias for the chain references the Crypto instance's alias.
/// impl ChainCryptoInstanceAlias for Ethereum {
///     type Instance = <EvmCrypto as PalletInstanceAlias>::Instance;
/// }
/// ```
#[macro_export]
macro_rules! impl_instance_alias_traits {
	( $( $crypto:ty => { $( $chain:ty ),+ } ),+ $(,)? ) => {
		$(
			impl ChainCryptoInstanceAlias for $crypto {
				type Instance = <$crypto as $crate::instances::PalletInstanceAlias>::Instance;
					const TYPE_INFO_SUFFIX: &'static str = <$crypto as $crate::instances::PalletInstanceAlias>::TYPE_INFO_SUFFIX;
			}
			$(
				impl ChainInstanceAlias for $chain {
					type Instance = <$chain as $crate::instances::PalletInstanceAlias>::Instance;
					const TYPE_INFO_SUFFIX: &'static str = <$chain as $crate::instances::PalletInstanceAlias>::TYPE_INFO_SUFFIX;
				}
				impl ChainCryptoInstanceAlias for $chain {
					type Instance = <$crypto as $crate::instances::PalletInstanceAlias>::Instance;
					const TYPE_INFO_SUFFIX: &'static str = <$crypto as $crate::instances::PalletInstanceAlias>::TYPE_INFO_SUFFIX;
				}
			)+
		)+
	};
}

decl_instance_aliases!(
	Ethereum => EthereumInstance, Instance1,
	Polkadot => PolkadotInstance, Instance2,
	PolkadotCrypto => PolkadotCryptoInstance, Instance2,
	Bitcoin => BitcoinInstance, Instance3,
	BitcoinCrypto => BitcoinCryptoInstance, Instance3,
	Arbitrum => ArbitrumInstance, Instance4,
	EvmCrypto => EvmInstance, Instance16,
	Solana => SolanaInstance, Instance5,
	SolanaCrypto => SolanaCryptoInstance, Instance5,
	NoneChain => NoneChainInstance, (),
	NoneChainCrypto => NoneChainCryptoInstance, (),
	AnyChain => AnyChainInstance, (),
);

impl_instance_alias_traits!(
	EvmCrypto => { Ethereum, Arbitrum },
	BitcoinCrypto => { Bitcoin },
	PolkadotCrypto => { Polkadot },
	SolanaCrypto => { Solana },
	NoneChainCrypto => { NoneChain, AnyChain },
);
