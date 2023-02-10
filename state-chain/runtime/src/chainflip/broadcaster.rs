use core::marker::PhantomData;

use crate::{EthereumBroadcaster, PolkadotBroadcaster};
use cf_chains::{
	any::{AnyChainApi, AnyGovKey},
	eth::api::EthereumApi,
	AnyChain, ChainCrypto, Ethereum, ForeignChain, Polkadot, SetAggKeyWithAggKey,
	SetGovKeyWithAggKey,
};
use cf_traits::{BroadcastAnyChainGovKey, Broadcaster};
use sp_std::vec::Vec;

use super::{DotEnvironment, EthEnvironment};

pub enum AnyApi {
	Ethereum(EthereumApi<EthEnvironment>),
	Polkadot(PolkadotApi<DotEnvironment>),
}

impl SetGovKeyWithAggKey<AnyChain> for AnyApi {
	fn new_unsigned(maybe_old_key: Option<AnyGovKey>, new_key: AnyGovKey) -> Result<Self, ()> {
		match (maybe_old_key, new_key) {
			AnyGovKey::Ethereum(key) =>
				<Ethereum as SetGovKeyWithAggKey>::new_unsigned(maybe_old_key, new_key),
			AnyGovKey::Polkadot(key) =>
				<Polkadot as SetGovKeyWithAggKey>::new_unsigned(maybe_old_key, new_key),
		}
	}
}

impl Broadcaster<AnyChain> for GovKeyBroadcaster {
	type ApiCall = AnyApi;

	fn threshold_sign_and_broadcast<C: ChainCrypto>(
		api_call: Self::ApiCall,
	) -> cf_primitives::BroadcastId {
		match api_call {
			AnyChainApi::Ethereum(eth_api_call) =>
				<EthereumBroadcaster as Broadcaster<Ethereum>>::threshold_sign_and_broadcast(
					eth_api_call,
				),
			AnyChainApi::Polkadot(dot_api_call) =>
				<PolkadotBroadcaster as Broadcaster<Polkadot>>::threshold_sign_and_broadcast(
					dot_api_call,
				),
		}
	}
}
