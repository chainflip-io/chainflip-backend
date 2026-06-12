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
use cf_chains::{witness_period::BlockWitnessRange, ChainWitnessConfig};
use core::ops::RangeInclusive;
use itertools::Either;
use sp_core::H256;
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet, vec_deque::VecDeque},
	vec::Vec,
};

/// A type which can be validated.
pub trait Validate {
	type Error: sp_std::fmt::Debug + PartialEq;
	fn is_valid(&self) -> Result<(), Self::Error>;
}

#[duplicate::duplicate_item(Type; [ () ]; [ bool ]; [ char ]; [ u8 ]; [ u16 ]; [ u32 ]; [ u64 ]; [ usize ] ; [ H256 ] ; [ sp_std::time::Duration ])]
impl Validate for Type {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

#[duplicate::duplicate_item(Container; [ Vec ]; [ VecDeque ]; [ BTreeSet ]; [ Option ]; )]
impl<A: Validate> Validate for Container<A> {
	type Error = A::Error;

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.iter().try_for_each(Validate::is_valid)
	}
}

impl<T> Validate for sp_std::marker::PhantomData<T> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<A: Validate, B: Validate> Validate for BTreeMap<A, B> {
	type Error = Either<A::Error, B::Error>;

	fn is_valid(&self) -> Result<(), Self::Error> {
		for (k, v) in self {
			k.is_valid().map_err(Either::Left)?;
			v.is_valid().map_err(Either::Right)?;
		}
		Ok(())
	}
}

#[cfg(test)]
impl Validate for String {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<A: Validate> Validate for RangeInclusive<A> {
	type Error = A::Error;

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.start().is_valid()?;
		self.end().is_valid()?;
		Ok(())
	}
}

impl<A, B: sp_std::fmt::Debug + Clone + PartialEq> Validate for Result<A, B> {
	type Error = B;

	fn is_valid(&self) -> Result<(), Self::Error> {
		match self {
			Ok(_) => Ok(()),
			Err(err) => Err(err.clone()),
		}
	}
}

impl<C: ChainWitnessConfig> Validate for BlockWitnessRange<C> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.check_is_valid()
	}
}
