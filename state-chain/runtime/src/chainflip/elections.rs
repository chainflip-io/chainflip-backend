use codec::{Decode, Encode};
use derive_where::derive_where;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

/// Type which can be used to implement hooks. It implements
/// all required bounds without requiring any bounds on the
/// `Tag` type parameter.
#[derive_where(Default, Debug, Clone, PartialEq, Eq;)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
#[codec(encode_bound())]
#[serde(bound = "")]
#[scale_info(skip_type_params(Tag))]
pub struct StatelessHookFor<Tag> {
	_phantom: sp_std::marker::PhantomData<Tag>,
}

/// Type which can be used for implementing traits that
/// contain only type definitions, as used in many parts of
/// the state machine based electoral systems.
#[derive_where(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord;)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
#[codec(encode_bound())]
#[serde(bound = "")]
#[scale_info(skip_type_params(Tag))]
pub struct TypesFor<Tag> {
	_phantom: sp_std::marker::PhantomData<Tag>,
}

macro_rules! impls {
    (for $name:ty: $(#[doc = $doc_text:tt])? $trait:ty {$($trait_impl:tt)*} $($rest:tt)*) => {
        $(#[doc = $doc_text])?
        impl $trait for $name {
            $($trait_impl)*
        }
        impls!{for $name: $($rest)*}
    };
    (for $name:ty:) => {}
}
