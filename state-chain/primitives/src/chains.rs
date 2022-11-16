use super::*;

pub mod assets;

macro_rules! chains {
	( $( $chain:ident ),+ ) => {
		$(
			#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
			pub struct $chain;

			impl AsRef<ForeignChain> for $chain {
				fn as_ref(&self) -> &ForeignChain {
					&ForeignChain::$chain
				}
			}
		)+

		#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
		#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
		pub enum ForeignChain {
			$(
				$chain,
			)+
		}
	}
}

/// Can be any Chain.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct AnyChain;

chains! {
	Ethereum,
	Polkadot
}

#[test]
fn test_chains() {
	assert_eq!(Ethereum.as_ref(), &ForeignChain::Ethereum);
	assert_eq!(Polkadot.as_ref(), &ForeignChain::Polkadot);
}

// Data storage that can hold any Account Id.
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo, MaxEncodedLen)]
pub struct AnyChainAccount([u8; 32]);

impl From<[u8; 32]> for AnyChainAccount {
	fn from(data: [u8; 32]) -> Self {
		AnyChainAccount(data)
	}
}

impl From<AnyChainAccount> for H160 {
	fn from(account: AnyChainAccount) -> Self {
		let mut data = [0u8; 20];
		data.clone_from_slice(&account.0[0..20]);
		data.into()
	}
}

impl From<AnyChainAccount> for AccountId32 {
	fn from(account: AnyChainAccount) -> Self {
		account.into()
	}
}

impl From<H160> for AnyChainAccount {
	fn from(account: H160) -> Self {
		let mut data = [0u8; 32];
		data.clone_from_slice(&account.to_fixed_bytes()[..]);
		data.into()
	}
}

impl From<[u8; 20]> for AnyChainAccount {
	fn from(account: [u8; 20]) -> Self {
		let mut data = [0u8; 32];
		data.clone_from_slice(&account[..]);
		data.into()
	}
}

impl From<AccountId32> for AnyChainAccount {
	fn from(account: AccountId32) -> Self {
		AnyChainAccount(account.into())
	}
}

impl TryFrom<ForeignChainAddress> for AnyChainAccount {
	type Error = ();
	fn try_from(value: ForeignChainAddress) -> Result<Self, ()> {
		Ok(match value {
			ForeignChainAddress::Eth(eth_addr) => eth_addr.into(),
			ForeignChainAddress::Dot(dot_addr) => dot_addr.into(),
		})
	}
}

impl From<AnyChainAccount> for ForeignChainAddress {
	fn from(account: AnyChainAccount) -> Self {
		account.0.into()
	}
}
