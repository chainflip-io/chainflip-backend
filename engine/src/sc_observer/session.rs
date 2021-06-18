use codec::Encode;
use frame_support::Parameter;
use sp_runtime::traits::{Member, OpaqueKeys};
use std::{fmt::Debug, marker::PhantomData};
use substrate_subxt::module;
use substrate_subxt::{system::System, Call, Event, Store};

/// Impls `Default::default` for some types that have a `_runtime` field of type
/// `PhantomData` as their only field.
macro_rules! default_impl {
    ($name:ident) => {
        impl<T: Session> Default for $name<T> {
            fn default() -> Self {
                Self {
                    _runtime: PhantomData,
                }
            }
        }
    };
}

/// The trait needed for this module.
#[module]
pub trait Session: System {
    #![event_alias(OpaqueTimeSlot = Vec<u8>)]
    #![event_alias(SessionIndex = u32)]

    /// The validator account identifier type for the runtime.
    type ValidatorId: Parameter + Debug + Ord + Default + Send + Sync + 'static;

    /// The keys.
    type Keys: OpaqueKeys + Member + Parameter + Default;
}

/// The current set of validators.
#[derive(Encode, Store, Debug)]
pub struct ValidatorsStore<T: Session> {
    #[store(returns = Vec<<T as Session>::ValidatorId>)]
    /// Marker for the runtime
    pub _runtime: PhantomData<T>,
}

default_impl!(ValidatorsStore);

/// Set the session keys for a validator.
#[derive(Encode, Call, Debug)]
pub struct SetKeysCall<T: Session> {
    /// The keys
    pub keys: T::Keys,
    /// The proof. This is not currently used and can be set to an empty vector.
    pub proof: Vec<u8>,
}
