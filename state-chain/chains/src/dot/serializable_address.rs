use super::*;
use frame_support::sp_runtime::AccountId32;

#[derive(Debug, Clone)]
pub struct SubstrateNetworkAddress {
	format_specifier: ss58_registry::Ss58AddressFormat,
	account_id: AccountId32,
}

impl SubstrateNetworkAddress {
	pub fn polkadot(account_id: impl Into<AccountId32>) -> Self {
		Self {
			format_specifier: ss58_registry::Ss58AddressFormatRegistry::PolkadotAccount.into(),
			account_id: account_id.into(),
		}
	}
}

impl serde::Serialize for SubstrateNetworkAddress {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		use sp_core::crypto::Ss58Codec;
		serializer.serialize_str(&self.account_id.to_ss58check_with_version(self.format_specifier))
	}
}

impl<'de> serde::Deserialize<'de> for SubstrateNetworkAddress {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		use sp_core::crypto::Ss58Codec;
		let s = String::deserialize(deserializer)?;
		<AccountId32 as Ss58Codec>::from_ss58check_with_version(&s)
			.map(|(account_id, format_specifier)| Self { format_specifier, account_id })
			.map_err(|_| serde::de::Error::custom("Invalid SS58 address"))
	}
}

impl TryFrom<SubstrateNetworkAddress> for PolkadotAccountId {
	type Error = sp_core::crypto::PublicError;

	fn try_from(substrate_address: SubstrateNetworkAddress) -> Result<Self, Self::Error> {
		if substrate_address.format_specifier ==
			ss58_registry::Ss58AddressFormatRegistry::PolkadotAccount.into()
		{
			Ok(Self::from_aliased(*substrate_address.account_id.as_ref()))
		} else {
			Err(sp_core::crypto::PublicError::FormatNotAllowed)
		}
	}
}

impl From<PolkadotAccountId> for SubstrateNetworkAddress {
	fn from(account_id: PolkadotAccountId) -> Self {
		Self::polkadot(account_id.0)
	}
}
