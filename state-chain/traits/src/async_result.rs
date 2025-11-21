// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use codec::{Decode, Encode};
use frame_support::pallet_prelude::RuntimeDebug;
use scale_info::TypeInfo;

/// A result type for asynchronous operations.
#[derive(Clone, Copy, Default, RuntimeDebug, Encode, Decode, TypeInfo, PartialEq, Eq)]
pub enum AsyncResult<R> {
	/// Result is ready.
	Ready(R),
	/// Result is requested but not available. (still being generated)
	Pending,
	/// Result is void. (not yet requested or has already been used)
	#[default]
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

	pub fn is_ready(&self) -> bool {
		matches!(self, AsyncResult::Ready(_))
	}

	pub fn unwrap(self) -> R {
		match self {
			AsyncResult::Ready(r) => r,
			_ => panic!("AsyncResult not Ready!"),
		}
	}

	pub fn replace_inner<S>(self, inner: S) -> AsyncResult<S> {
		match self {
			AsyncResult::Ready(_) => AsyncResult::Ready(inner),
			AsyncResult::Pending => AsyncResult::Pending,
			AsyncResult::Void => AsyncResult::Void,
		}
	}
}

impl<R> From<R> for AsyncResult<R> {
	fn from(r: R) -> Self {
		AsyncResult::Ready(r)
	}
}
