use codec::{Decode, Encode};
use frame_support::RuntimeDebug;
use scale_info::TypeInfo;

/// A result type for asynchronous operations.
#[derive(Clone, Copy, RuntimeDebug, Encode, Decode, TypeInfo, PartialEq, Eq)]
pub enum AsyncResult<R> {
	/// Result is ready.
	Ready(R),
	/// Result is requested but not available. (still being generated)
	Pending,
	/// Result is void. (not yet requested or has already been used)
	Void,
}

impl<R> AsyncResult<R> {
	/// Returns `Ok(result: R)` if the `R` is ready, otherwise executes the supplied closure and
	/// returns the Err(closure_result: E).
	pub fn ready_or_else<E>(self, e: impl FnOnce(Self) -> E) -> Result<R, E> {
		match self {
			AsyncResult::Ready(s) => Ok(s),
			_ => Err(e(self)),
		}
	}
}

impl<R> Default for AsyncResult<R> {
	fn default() -> Self {
		Self::Void
	}
}

impl<R> From<R> for AsyncResult<R> {
	fn from(r: R) -> Self {
		AsyncResult::Ready(r)
	}
}
