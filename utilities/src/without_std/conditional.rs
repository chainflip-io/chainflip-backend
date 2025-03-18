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

pub enum Boolean<True, False> {
	True(True),
	False(False),
}

pub trait BooleanT {
	type True;
	type False;

	fn into_boolean(self) -> Boolean<Self::True, Self::False>;
}

impl<T> BooleanT for Option<T> {
	type True = T;
	type False = ();

	fn into_boolean(self) -> Boolean<Self::True, Self::False> {
		match self {
			Some(t) => Boolean::True(t),
			None => Boolean::False(()),
		}
	}
}
impl<T, E> BooleanT for Result<T, E> {
	type True = T;
	type False = E;

	fn into_boolean(self) -> Boolean<Self::True, Self::False> {
		match self {
			Ok(t) => Boolean::True(t),
			Err(e) => Boolean::False(e),
		}
	}
}
impl<'a, T> BooleanT for &'a Option<T> {
	type True = &'a T;
	type False = ();

	fn into_boolean(self) -> Boolean<Self::True, Self::False> {
		match self {
			Some(t) => Boolean::True(t),
			None => Boolean::False(()),
		}
	}
}
impl<'a, T, E> BooleanT for &'a Result<T, E> {
	type True = &'a T;
	type False = &'a E;

	fn into_boolean(self) -> Boolean<Self::True, Self::False> {
		match self {
			Ok(t) => Boolean::True(t),
			Err(e) => Boolean::False(e),
		}
	}
}
impl BooleanT for bool {
	type True = ();
	type False = ();

	fn into_boolean(self) -> Boolean<Self::True, Self::False> {
		if self {
			Boolean::True(())
		} else {
			Boolean::False(())
		}
	}
}

/// For handling cases of `if`` statements where you would like the two branches to return different
/// types, but that both implement the same trait
pub fn conditional<
	B: BooleanT,
	TrueFn: FnOnce(B::True) -> MappedTrue,
	FalseFn: FnOnce(B::False) -> MappedFalse,
	MappedTrue,
	MappedFalse,
>(
	bool: B,
	f: TrueFn,
	g: FalseFn,
) -> itertools::Either<MappedTrue, MappedFalse> {
	match bool.into_boolean() {
		Boolean::True(true_value) => itertools::Either::Left(f(true_value)),
		Boolean::False(false_value) => itertools::Either::Right(g(false_value)),
	}
}
