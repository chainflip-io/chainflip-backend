use crate::{Config, FailedForeignChainCalls};
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::{marker::PhantomData, prelude::*};

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		for (i, v) in FailedForeignChainCalls::<T, I>::drain()
			.filter(|(_, v)| !v.is_empty())
			.collect::<Vec<_>>()
		{
			FailedForeignChainCalls::<T, I>::insert(i, v);
		}

		Default::default()
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		mock_eth::{new_test_ext, Test},
		FailedForeignChainCall,
	};

	#[test]
	fn test_migration() {
		let call = |i| FailedForeignChainCall { broadcast_id: i, original_epoch: i };
		new_test_ext().execute_with(|| {
			FailedForeignChainCalls::<Test, _>::set(1, vec![call(1)]);
			FailedForeignChainCalls::<Test, _>::set(2, vec![]);
			FailedForeignChainCalls::<Test, _>::set(3, vec![call(2), call(3)]);
			FailedForeignChainCalls::<Test, _>::set(4, vec![]);

			assert!(FailedForeignChainCalls::<Test, _>::contains_key(2));
			assert!(FailedForeignChainCalls::<Test, _>::contains_key(4));

			Migration::<Test, _>::on_runtime_upgrade();

			assert_eq!(FailedForeignChainCalls::<Test, _>::get(1), vec![call(1)]);
			assert!(!FailedForeignChainCalls::<Test, _>::contains_key(2));
			assert_eq!(FailedForeignChainCalls::<Test, _>::get(3), vec![call(2), call(3)]);
			assert!(!FailedForeignChainCalls::<Test, _>::contains_key(4));
		});
	}
}
