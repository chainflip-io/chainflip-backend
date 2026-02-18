#[cfg(test)]
use proptest::prelude::{Arbitrary, Strategy};

use codec::{Decode, DecodeWithMemTracking, Encode};
use derive_where::derive_where;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

/// Adds the type parameters to all given implementatios
macro_rules! implementations {
	([$($Name:tt)*], [$($Parameters:tt)*], impl { $($Implementation:tt)* } $($rest:tt)* ) => {

		impl <$($Parameters)*> $($Name)* {
			$($Implementation)*
		}
		crate::electoral_systems::state_machine::core::implementations!  {
			[$($Name)*], [$($Parameters)*], $($rest)*
		}
	};

	([$($Name:tt)*], [$($Parameters:tt)*], impl$(<$($TraitParamName:ident: $TraitParamPath:path),*>)? $Trait:path { $($TraitDef:tt)* } $($rest:tt)* ) => {

		impl <$($Parameters)*, $($($TraitParamName: $TraitParamPath),*)?> $Trait for $($Name)* {
			$($TraitDef)*
		}

		crate::electoral_systems::state_machine::core::implementations!  {
			[$($Name)*], [$($Parameters)*], $($rest)*
		}

	};

	([$($Name:tt)*], [$($Parameters:tt)*],) => {}
}
pub(crate) use implementations;

/// Derive error enum cases from a struct or enum definition
macro_rules! derive_error_enum {
	($Error:ident [$($ParamsDef:tt)*], struct { $( $(#[doc = $doc_text:tt])* $vis:vis $Field:ident: $Type:ty, )* } { $( $property:ident ),* }
	) => {

		#[derive_where::derive_where(Debug, PartialEq)]
		#[allow(clippy::allow_attributes)]
		#[allow(non_camel_case_types)]
		pub enum $Error<$($ParamsDef)*> {

			$(
				$Field(<$Type as cf_traits::Validate>::Error),
			)*

			$(
				$property,
			)*
		}

	};

	($Error:ident [$($ParamName:ident: $ParamType:tt),*], enum { $( $anything:tt )* } { $( $property:ident ),* }
	) => {

		#[derive_where::derive_where(Debug, PartialEq; )]
		#[allow(clippy::allow_attributes)]
		#[allow(non_camel_case_types)]
		pub enum $Error<$($ParamName: $ParamType),*> {

			// TODO call validate on all enum cases
			// Currently we only have a single enum which would profit, and we do it manually there.

			$(
				$property,
			)*

			PhantomCase(sp_std::marker::PhantomData<($($ParamName,)*)>)
		}

	};
}
pub(crate) use derive_error_enum;

macro_rules! derive_validation_statements {
	($this:ident, $Error:ident, struct { $( $(#[doc = $doc_text:tt])* $vis:vis $Field:ident: $Type:ty, )* }
	) => {
		$(
			$this.$Field.is_valid().map_err($Error::$Field)?;
		)*
	};

	($Error:ident, $this:ident, enum { $( $anything:tt )* }
	) => {
	};
}
pub(crate) use derive_validation_statements;

/// Syntax sugar for adding validation code to types with validity requirements
macro_rules! defx {
	(
		$(#[$($Attributes:tt)*])*
		pub $def:tt $Name:tt [$($ParamName:ident $(: $ParamType:tt)?),*] {
			$($Definition:tt)*
		}
		validate $this:ident (else $Error:ident) {
			$($prop_name:ident : $prop:expr),*

			$(,
			( where
				$(
					$prop_var:ident = $prop_var_value:expr
				),*
			))?
		}

		$($rest:tt)*
	) => {

		crate::electoral_systems::state_machine::core::derive_error_enum!{$Error [ $($ParamName: $($ParamType)?),* ], $def { $($Definition)* } { $($prop_name),* } }


		cf_utilities::macros::derive_common_traits!{
			$(
				#[$($Attributes)*]
			)*
			pub $def $Name<$($ParamName: $($ParamType)?),*> {
				$($Definition)*
			}
		}

		impl<$($ParamName: $($ParamType)?),*> cf_traits::Validate for $Name<$($ParamName),*> {

			type Error = $Error<$($ParamName),*>;

			fn is_valid(&self) -> Result<(), Self::Error> {
				let $this = self;

				$(
					$(
						let $prop_var = $prop_var_value;
					)*
				)?

				crate::electoral_systems::state_machine::core::derive_validation_statements!($this, $Error, $def { $($Definition)* } );

				$(
					frame_support::ensure!($prop, $Error::$prop_name);
				)*
				Ok(())
			}
		}

		crate::electoral_systems::state_machine::core::implementations!{[$Name<$($ParamName),*>], [ $($ParamName: $($ParamType)?),* ], $($rest)*}
	};
}
pub(crate) use defx;

/// Type which can be used for implementing traits that
/// contain only type definitions, as used in many parts of
/// the state machine based electoral systems.
#[derive_where(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord;)]
#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo, Deserialize, Serialize)]
#[codec(encode_bound())]
#[serde(bound = "")]
#[scale_info(skip_type_params(Tag))]
#[allow(clippy::allow_attributes)]
#[allow(dead_code)]
pub(crate) struct TypesFor<Tag> {
	_phantom: sp_std::marker::PhantomData<Tag>,
}

impl<Tag> cf_traits::Validate for TypesFor<Tag> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

#[cfg(test)]
impl<Tag: Sync + Send> Arbitrary for TypesFor<Tag> {
	type Parameters = ();

	fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
		use proptest::prelude::Just;
		Just(TypesFor { _phantom: Default::default() })
	}

	type Strategy = impl Strategy<Value = Self> + Clone + Sync + Send;
}
