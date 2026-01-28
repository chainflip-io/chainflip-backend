/// Adds #[derive] statements for commonly used traits. These are currently: Debug, Clone,
/// PartialEq, Eq, Encode, Decode, Serialize, Deserialize
#[macro_export]
macro_rules! derive_common_traits {
	($($Definition:tt)*) => {
		#[derive(
			Debug, Clone, PartialEq, Eq, Encode, Decode,
		)]
		#[derive(Deserialize, Serialize)]
		#[serde(bound(deserialize = "", serialize = ""))]
		$($Definition)*
	};
}
pub use derive_common_traits;

/// Adds #[derive] statements for commonly used traits, including `Validate`. Automatically
/// generates the body of the struct, containing just a PhantomData with all type variables.
/// Use like:
///
/// use utilities::macros::define_empty_struct;
/// define_empty_struct! {
///     struct SomeStruct<A: Ord, B: 'static>;
/// }
#[macro_export]
macro_rules! define_empty_struct {
	(
		[$name:ident $(: $path:path)?, $($rest:tt)*]
		[$($names:tt)*]
		[$($names_and_bounds:tt)*]
		$vis:vis struct $struct_name:ident
	) => {
		cf_utilities::define_empty_struct!{
			[$($rest)*]
			[$($names)* $name, ]
			[$($names_and_bounds)* $name $(:$path)?, ]
			$vis struct $struct_name
		}
	};
	(
		[$name:ident: $l:lifetime, $($rest:tt)*]
		[$($names:tt)*]
		[$($names_and_bounds:tt)*]
		$vis:vis struct $struct_name:ident
	) => {
		cf_utilities::define_empty_struct!{
			[$($rest)*]
			[$($names)* $name, ]
			[$($names_and_bounds)* $name:$l, ]
			$vis struct $struct_name
		}
	};

	// handling the last entry
	( [$name:ident $(: $path:path)? >;]  $($rest:tt)* ) => { cf_utilities::define_empty_struct!{ [ $name $(:$path)?, >; ] $($rest)* }};
	( [$name:ident: $l:lifetime >;] $($rest:tt)* ) => { cf_utilities::define_empty_struct!{ [ $name:$l, >; ] $($rest)* }};


	// the main branch
	(
		[>;]
		[$($names:tt)*]
		[$($names_and_bounds:tt)*]
		$vis:vis struct $struct_name:ident
	) => {
		cf_utilities::derive_common_traits!{
			#[derive(TypeInfo, DefaultNoBound)]
			#[scale_info(skip_type_params(T, I))]
			$vis struct $struct_name<$($names_and_bounds)*>
			(
				sp_std::marker::PhantomData
				<
				($($names)*)
				>
			);

			impl<$($names_and_bounds)*> Validate for $struct_name<$($names)*> {
				type Error = ();

				fn is_valid(&self) -> Result<(), Self::Error> {
					Ok(())
				}
			}
		}
	};
	(
		$vis:vis struct $struct_name:ident<$($rest:tt)*
	) => {
		cf_utilities::define_empty_struct!{
			[$($rest)*]
			[]
			[]
			$vis struct $struct_name
		}
	};
}
pub use define_empty_struct;

/// Syntax sugar for implementing multiple traits for a single type.
///
/// Example use:
///
/// impls! {
///     for u8:
///     Clone {
///         ...
///     }
///     Copy {
///         ...
///     }
///     Default {
///         ...
///     }
/// }
#[macro_export]
macro_rules! impls {
    (for $name:ty $(where ($($bounds:tt)*))? :
	$(#[doc = $doc_text:tt])? impl $($trait:ty)?  $(where ($($trait_bounds:tt)*))? {$($trait_impl:tt)*}
	$($rest:tt)*
	) => {
        $(#[doc = $doc_text])?
        impl$(<$($bounds)*>)? $($trait for)? $name
		$(where $($trait_bounds)*)?
		{
            $($trait_impl)*
        }
        impls!{for $name $(where ($($bounds)*))? : $($rest)*}
    };
    (for $name:ty $(where ($($bounds:tt)*))? :) => {}
}
