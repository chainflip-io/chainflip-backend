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
