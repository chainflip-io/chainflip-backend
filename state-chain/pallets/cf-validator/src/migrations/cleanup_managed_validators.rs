use crate::{Config, ManagedValidators, OperatorChoice};
use cf_primitives::AccountRole;
use cf_traits::AccountRoleRegistry;
use frame_support::{traits::UncheckedOnRuntimeUpgrade, weights::Weight};
use sp_std::vec::Vec;

#[cfg(feature = "try-runtime")]
use frame_support::sp_runtime::DispatchError;

pub struct CleanupManagedValidators<T: Config>(sp_std::marker::PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for CleanupManagedValidators<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🧹 Starting ManagedValidators cleanup migration");

		// Collect all operator keys first to avoid issues with iteration and modification
		let operator_keys: Vec<_> = ManagedValidators::<T>::iter_keys().collect();

		for operator in operator_keys {
			ManagedValidators::<T>::mutate(&operator, |validators| {
				validators.retain(|validator_id| {
					T::AccountRoleRegistry::has_account_role(validator_id, AccountRole::Validator)
				});
			});
		}

		for (v, _) in OperatorChoice::<T>::iter() {
			if !T::AccountRoleRegistry::has_account_role(&v, AccountRole::Validator) {
				OperatorChoice::<T>::remove(v);
			}
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		// Verify ManagedValidators: no operator should manage a validator without Validator role
		for (operator, managed_validators) in ManagedValidators::<T>::iter() {
			for validator in managed_validators.iter() {
				if !T::AccountRoleRegistry::has_account_role(validator, AccountRole::Validator) {
					log::error!(
						"Post-upgrade: Found invalid validator {:?} managed by operator {:?}",
						validator,
						operator
					);
					return Err("ManagedValidators contains validator without Validator role".into());
				}
			}
		}

		// Verify OperatorChoice: all validators must have Validator role
		for (validator, operator) in OperatorChoice::<T>::iter() {
			if !T::AccountRoleRegistry::has_account_role(&validator, AccountRole::Validator) {
				log::error!(
					"Post-upgrade: Found invalid validator {:?} in OperatorChoice (operator: {:?})",
					validator,
					operator
				);
				return Err("OperatorChoice contains validator without Validator role".into());
			}
		}

		log::info!(
			"✅ Post-upgrade validation passed for both ManagedValidators and OperatorChoice"
		);
		Ok(())
	}
}
