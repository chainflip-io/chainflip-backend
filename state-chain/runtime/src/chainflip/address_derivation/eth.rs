use crate::{Environment, EthEnvironment};
use cf_chains::{
	eth::{api::EthEnvironmentProvider, deposit_address::get_create_2_address, EthereumChannelId},
	Chain, Ethereum,
};
use cf_primitives::{chains::assets::eth, ChannelId};
use cf_traits::{AddressDerivationApi, DepositChannel};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::H160;
use sp_runtime::DispatchError;

use super::AddressDerivation;

impl AddressDerivationApi<Ethereum> for AddressDerivation {
	fn generate_address(
		source_asset: eth::Asset,
		channel_id: ChannelId,
	) -> Result<<Ethereum as Chain>::ChainAccount, DispatchError> {
		Ok(get_create_2_address(
			Environment::eth_vault_address(),
			EthEnvironment::token_address(source_asset).map(|address| address.to_fixed_bytes()),
			channel_id,
		)
		.into())
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Copy, Debug)]
pub enum DeploymentStatus {
	Deployed,
	Pending,
	Undeployed,
}

#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Copy, Debug)]
pub struct EthereumDepositAddress {
	pub channel_id: u64,
	pub address: H160,
	pub asset: eth::Asset,
	pub deployment_status: DeploymentStatus,
}

impl DepositChannel<Ethereum> for EthereumDepositAddress {
	type AddressDerivation = AddressDerivation;

	fn new(channel_id: u64, asset: <Ethereum as Chain>::ChainAsset) -> Result<Self, DispatchError> {
		let address = <AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
			asset, channel_id,
		)?;
		Ok(Self { address, channel_id, deployment_status: DeploymentStatus::Undeployed, asset })
	}

	fn get_address(&self) -> Self::Address {
		self.address
	}

	fn get_channel_id(&self) -> u64 {
		self.channel_id
	}

	fn get_asset(&self) -> <Ethereum as Chain>::ChainAsset {
		self.asset
	}

	fn get_fetch_id(&self) -> Option<Self::DepositFetchId> {
		match self.deployment_status {
			DeploymentStatus::Undeployed => Some(EthereumChannelId::Undeployed(self.channel_id)),
			DeploymentStatus::Pending => None,
			DeploymentStatus::Deployed => Some(EthereumChannelId::Deployed(self.address)),
		}
	}

	/// The address needs to be marked as Pending until the fetch is made.
	fn on_fetch_broadcast(&mut self) -> bool {
		match self.deployment_status {
			DeploymentStatus::Undeployed => {
				self.deployment_status = DeploymentStatus::Pending;
				true
			},
			_ => false,
		}
	}

	/// Undeployed Addresses should not be recycled.
	/// Other address types *can* be recycled.
	fn maybe_recycle(self) -> Option<Self> {
		if self.deployment_status == DeploymentStatus::Undeployed {
			None
		} else {
			Some(Self { deployment_status: DeploymentStatus::Deployed, ..self })
		}
	}

	/// A completed fetch should be in either the pending or deployed state. Confirmation of a fetch
	/// implies that the address is now deployed.
	fn on_fetch_completed(&mut self) -> bool {
		Self {
			deployment_status: match self.deployment_status {
				DeploymentStatus::Pending | DeploymentStatus::Deployed =>
					DeploymentStatus::Deployed,
				DeploymentStatus::Undeployed => {
					#[cfg(test)]
					{
						panic!("Cannot finalize fetch to an undeployed address")
					}
					log::error!("Cannot finalize fetch to an undeployed address");
					DeploymentStatus::Undeployed
				},
			},
			..self
		}
	}
}

#[test]
fn test_address_generation() {
	use crate::Runtime;
	use cf_chains::Ethereum;
	use cf_primitives::chains::assets::eth::Asset;
	use pallet_cf_environment::EthereumSupportedAssets;

	frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
		// Expect address generation to be successfully for native ETH
		assert!(<AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
			eth::Asset::Eth,
			1
		)
		.is_ok());
		// The genesis build is not running, so we have to add it manually
		EthereumSupportedAssets::<Runtime>::insert(Asset::Flip, [0; 20]);
		// Expect address generation to be successfully for ERC20 Flip token
		assert!(<AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
			eth::Asset::Flip,
			1
		)
		.is_ok());

		// Address derivation for Dot is currently unimplemented.
		// Expect address generation to return an error for unsupported assets. Because we are
		// running a test gainst ETH the DOT asset will be always unsupported.
		// assert!(AddressDerivation::generate_address(Asset::Dot, 1).is_err());
	});
}
