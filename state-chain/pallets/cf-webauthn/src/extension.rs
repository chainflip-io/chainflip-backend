use crate::{Config, Credentials, Pallet, WeightInfo};
use cf_webauthn::Assertion;
use codec::{Decode, DecodeWithMemTracking, Encode};
use core::marker::PhantomData;
use frame_support::{pallet_prelude::TransactionSource, traits::OriginTrait};
use scale_info::TypeInfo;
use sp_io::hashing::blake2_256;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{AsTransactionAuthorizedOrigin, DispatchInfoOf, Dispatchable, TransactionExtension},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
	AccountId32, Weight,
};

/// Authorises a general (v5) transaction with a registered WebAuthn credential.
///
/// Must precede every other authorising extension. The signed message is
/// `blake2_256(inherited_implication)`, i.e. the extension version, the call and all following
/// extensions' explicit and implicit data, so nonce, era, genesis and spec version are covered
/// without being re-implemented here.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct CheckWebAuthn<T>(pub Option<(AccountId32, Assertion)>, PhantomData<T>);

impl<T> CheckWebAuthn<T> {
	pub fn disabled() -> Self {
		Self(None, PhantomData)
	}

	pub fn signed(account_id: AccountId32, assertion: Assertion) -> Self {
		Self(Some((account_id, assertion)), PhantomData)
	}
}

impl<T> core::fmt::Debug for CheckWebAuthn<T> {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "CheckWebAuthn({:?})", self.0.as_ref().map(|(account_id, _)| account_id))
	}
}

impl<T: Config + Send + Sync> TransactionExtension<T::RuntimeCall> for CheckWebAuthn<T>
where
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin: AsTransactionAuthorizedOrigin,
{
	const IDENTIFIER: &'static str = "CheckWebAuthn";
	type Implicit = ();
	type Val = ();
	type Pre = ();

	fn weight(&self, _call: &T::RuntimeCall) -> Weight {
		if self.0.is_some() {
			T::WeightInfo::check_webauthn()
		} else {
			Weight::zero()
		}
	}

	fn validate(
		&self,
		mut origin: <T::RuntimeCall as Dispatchable>::RuntimeOrigin,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_self_implicit: (),
		inherited_implication: &impl Encode,
		_source: TransactionSource,
	) -> Result<
		(ValidTransaction, Self::Val, <T::RuntimeCall as Dispatchable>::RuntimeOrigin),
		TransactionValidityError,
	> {
		let Some((account_id, assertion)) = &self.0 else {
			return Ok((Default::default(), (), origin))
		};
		if origin.is_transaction_authorized() {
			return Err(InvalidTransaction::BadSigner.into())
		}
		// Storage lookup before the comparatively expensive signature check, so unregistered
		// signers are rejected cheaply.
		let credential = Credentials::<T>::get(account_id).ok_or(InvalidTransaction::BadSigner)?;
		let policy = Pallet::<T>::policy().ok_or(InvalidTransaction::BadSigner)?;
		cf_webauthn::verify_assertion(
			&credential.public_key,
			&inherited_implication.using_encoded(blake2_256),
			assertion,
			&policy,
		)
		.map_err(|_| InvalidTransaction::BadProof)?;

		origin.set_caller_from_signed(account_id.clone());
		Ok((ValidTransaction::default(), (), origin))
	}

	impl_tx_ext_default!(T::RuntimeCall; prepare);
}
