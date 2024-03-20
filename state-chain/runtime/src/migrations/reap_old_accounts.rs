use crate::Runtime;
use cf_primitives::{AccountRole, FLIPPERINOS_PER_FLIP};
#[cfg(feature = "try-runtime")]
use codec::Decode;
use codec::Encode;
#[cfg(feature = "try-runtime")]
use frame_support::sp_runtime::DispatchError;
use frame_support::{
	traits::{HandleLifetime, OnRuntimeUpgrade, OriginTrait},
	weights::Weight,
};
use pallet_cf_account_roles::AccountRoles;
use pallet_cf_funding::PendingRedemptionInfo;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub struct Migration;

/// This migration reaps old accounts based on the following critera:
/// - The account has no FLIP balance.
/// - The account has no pending redemption.
/// - The account has only one provider and one consumer.
impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🪓 Reaping old accounts.");
		for (account_id, account) in pallet_cf_flip::Account::<Runtime>::iter() {
			if account.total() == 0u128 {
				let account_id_hex = hex::encode(&account_id.encode());
				if let Some(PendingRedemptionInfo { total, .. }) =
					pallet_cf_funding::PendingRedemptions::<Runtime>::get(&account_id)
				{
					log::info!(
						"💰 Account {}, has {}FLIP pending redemption. Skipping.",
						account_id_hex,
						total / FLIPPERINOS_PER_FLIP
					);
					continue;
				}
				let account_info = frame_system::Pallet::<Runtime>::account(&account_id);

				if account_info.providers == 1 && account_info.consumers == 1 {
					let _ = match AccountRoles::<Runtime>::get(&account_id) {
						Some(role) => match role {
							AccountRole::Unregistered => Err("Account is unregistered".into()),
							AccountRole::Validator => {
								// To compensate for the consumer that we *should* have added.
								frame_system::Consumer::<Runtime>::created(&account_id).expect("Can only fail if no providers or too many consumers, checked above.");
								pallet_cf_validator::Pallet::<Runtime>::deregister_as_validator(
									OriginTrait::signed(account_id.clone()),
								)
							},
							AccountRole::LiquidityProvider => {
								// To compensate for the consumer that we *should* have added.
								frame_system::Consumer::<Runtime>::created(&account_id).expect("Can only fail if no providers or too many consumers, checked above.");
								pallet_cf_lp::Pallet::<Runtime>::deregister_lp_account(
									OriginTrait::signed(account_id.clone()),
									false,
								)
							},
							AccountRole::Broker => {
								// To compensate for the consumer that we *should* have added.
								frame_system::Consumer::<Runtime>::created(&account_id).expect("Can only fail if no providers or too many consumers, checked above.");
								pallet_cf_swapping::Pallet::<Runtime>::deregister_as_broker(
									OriginTrait::signed(account_id.clone()),
									false,
								)
							},
						}
						.inspect_err(|e| {
							log::error!(
								"❗️ Failed to deregister {:?} account {}: {:?}",
								role,
								account_id_hex,
								e
							)
						})
						.inspect(|_| {
							log::info!("✅ Deregistered {} as {:?}", account_id_hex, role);
						}),
						None => {
							log::warn!("❗️ Account {} has no role", account_id_hex);
							Ok(())
						},
					}
					.and_then(|_| frame_system::Pallet::<Runtime>::dec_providers(&account_id))
					.inspect_err(|e| {
						log::error!(
							"❗️ Failed to decrement provider count for account {}: {:?}",
							account_id_hex,
							e
						)
					})
					.inspect(|_| {
						log::info!("💀 Reaped account {}.", account_id_hex);
					});
				} else {
					log::warn!("❗️ Expected provider and consumer count to be one for account {}, but got: {:?}", account_id_hex, account_info);
				}
			}
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let accounts_to_reap = pallet_cf_flip::Account::<Runtime>::iter()
			.filter_map(
				|(account_id, account)| {
					if account.total() == 0u128 {
						Some(account_id)
					} else {
						None
					}
				},
			)
			.collect::<Vec<_>>();
		Ok(accounts_to_reap.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use frame_support::ensure;

		let accounts_to_reap =
			Vec::<<Runtime as frame_system::Config>::AccountId>::decode(&mut &state[..])
				.map_err(|_| DispatchError::Other("Failed to decode accounts to reap"))?;
		for account_id in accounts_to_reap {
			ensure!(
				pallet_cf_funding::PendingRedemptions::<Runtime>::contains_key(&account_id) ||
					(!pallet_cf_flip::Account::<Runtime>::contains_key(&account_id) &&
						!frame_system::Pallet::<Runtime>::account_exists(&account_id)),
				"Account was not reaped",
			);
		}
		Ok(())
	}
}
