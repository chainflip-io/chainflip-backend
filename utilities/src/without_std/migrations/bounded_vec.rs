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

use sp_core::bounded::BoundedVec;
use sp_std::vec::Vec;

use crate::{
	migrations::{
		basics::{vCurrent, Migration, Version},
		with_all_runtime_migrations, HasChangelog, HasGenericVariant, IsHistoricalType,
	},
	never::Never,
	type_introspection::HasTypeIntrospection,
};

use crate::migrations::primitives::{GenericMapMigration, MapMigration, VecMigrationFailed};

// ---- BoundedVec ----

#[derive(codec::Encode, codec::Decode, serde::Serialize, serde::Deserialize, Clone)]
pub struct WrappedBoundedVec<X, S: sp_core::Get<u32>>(pub BoundedVec<X, S>);

impl<X: sp_std::fmt::Debug, S: sp_core::Get<u32>> sp_std::fmt::Debug for WrappedBoundedVec<X, S> {
	fn fmt(&self, formatter: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		self.0.fmt(formatter)
	}
}

impl<X: scale_info::TypeInfo + 'static, S: sp_core::Get<u32> + 'static> scale_info::TypeInfo
	for WrappedBoundedVec<X, S>
{
	type Identity = <BoundedVec<X, S> as scale_info::TypeInfo>::Identity;

	fn type_info() -> scale_info::Type {
		<BoundedVec<X, S> as scale_info::TypeInfo>::type_info()
	}
}

#[cfg(all(feature = "proptest", feature = "std"))]
impl<X: proptest::arbitrary::Arbitrary, S: sp_core::Get<u32> + 'static>
	proptest::arbitrary::Arbitrary for WrappedBoundedVec<X, S>
{
	type Parameters = <Vec<X> as proptest::arbitrary::Arbitrary>::Parameters;

	fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
		use proptest::prelude::*;
		<Vec<X> as Arbitrary>::arbitrary_with(args)
			.prop_map(|values| Self(BoundedVec::truncate_from(values)))
	}

	type Strategy = impl proptest::strategy::Strategy<Value = Self>;
}

impl<X: HasTypeIntrospection, S: sp_core::Get<u32>> HasTypeIntrospection
	for WrappedBoundedVec<X, S>
{
	fn is_empty_type() -> bool {
		false
	}

	fn sample_all_shapes() -> Vec<Self> {
		BoundedVec::<X, S>::sample_all_shapes().into_iter().map(Self).collect()
	}
}

macro_rules! impl_changelog_for_bounded_vec {
    ($($migration:ident,)*) => {
        impl<X: HasChangelog, S: sp_core::Get<u32>> HasChangelog for BoundedVec<X, S> {
            type if_unspecified = MapMigration<(X::if_unspecified,)>;

            $(
                type $migration = MapMigration<(X::$migration,)>;
            )*
        }
    };
}
with_all_runtime_migrations! {impl_changelog_for_bounded_vec}

impl<X, S: sp_core::Get<u32>, V: Version, M: Migration<X, V>> Migration<WrappedBoundedVec<X, S>, V>
	for MapMigration<(M,)>
{
	type From = WrappedBoundedVec<M::From, S>;
	type ForwardsError = VecMigrationFailed<M::ForwardsError>;
	type BackwardsError = VecMigrationFailed<M::BackwardsError>;

	fn try_forwards(x: Self::From) -> Result<WrappedBoundedVec<X, S>, Self::ForwardsError> {
		let result =
			x.0.into_iter()
				.enumerate()
				.map(|(index, x)| {
					M::try_forwards(x).map_err(|error| VecMigrationFailed::Element { index, error })
				})
				.collect::<Result<Vec<_>, _>>()?;
		Ok(WrappedBoundedVec(BoundedVec::truncate_from(result)))
	}

	fn try_backwards(x: WrappedBoundedVec<X, S>) -> Result<Self::From, Self::BackwardsError> {
		let result = x
			.0
			.into_iter()
			.enumerate()
			.map(|(index, x)| {
				M::try_backwards(x).map_err(|error| VecMigrationFailed::Element { index, error })
			})
			.collect::<Result<Vec<_>, _>>()?;
		Ok(WrappedBoundedVec(BoundedVec::truncate_from(result)))
	}
}

impl<
		X,
		S: sp_core::Get<u32>,
		M: Migration<X, vCurrent, ForwardsError = Never, BackwardsError = Never>,
	> Migration<BoundedVec<X, S>, vCurrent> for GenericMapMigration<(M,)>
{
	type From = WrappedBoundedVec<M::From, S>;

	fn try_forwards(x: Self::From) -> Result<BoundedVec<X, S>, Self::ForwardsError> {
		let result = x.0.into_iter().map(M::try_forwards).collect::<Result<Vec<_>, _>>()?;
		Ok(BoundedVec::truncate_from(result))
	}

	fn try_backwards(x: BoundedVec<X, S>) -> Result<Self::From, Self::BackwardsError> {
		let result = x.into_iter().map(M::try_backwards).collect::<Result<Vec<_>, _>>()?;
		Ok(WrappedBoundedVec(BoundedVec::truncate_from(result)))
	}
}

impl<X: HasGenericVariant, S: sp_core::Get<u32>> HasGenericVariant for BoundedVec<X, S> {
	type GenericType = WrappedBoundedVec<X::GenericType, S>;
	type MigrationFromGeneric = GenericMapMigration<(X::MigrationFromGeneric,)>;
}
impl<X: IsHistoricalType, S: sp_core::Get<u32>> IsHistoricalType for BoundedVec<X, S> {
	type GetCurrentType = BoundedVec<X::GetCurrentType, S>;
}

impl<X: IsHistoricalType, S: sp_core::Get<u32>> IsHistoricalType for WrappedBoundedVec<X, S> {
	type GetCurrentType = BoundedVec<X::GetCurrentType, S>;
}
