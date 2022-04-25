use std::marker::PhantomData;

use cf_chains::{eth::api::EthereumNonce, ChainAbi};

/// A mock that just returns some constants for the EthereumNonce.
pub struct MockEthNonceProvider<T>(PhantomData<T>);

impl<T: ChainAbi> crate::NonceProvider<T> for MockEthNonceProvider<T>
where
	<T as ChainAbi>::Nonce: From<EthereumNonce>,
{
	fn next_nonce() -> <T as ChainAbi>::Nonce {
		EthereumNonce { key_manager_address: [0xcf; 20], chain_id: 31337, counter: 42 }.into()
	}
}
