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

use cf_utilities::migrations::{
	basics::{HasGenericVariant, HasVersion, IdentityMigration},
	v20100, v20200, v20300, HasChangelog,
};

use crate::Asset;

use super::assets::*;

// -------------- HasChangelog ---------------- //

impl<T: HasChangelog> HasChangelog for hub::AssetMap<T> {
	type if_unspecified = hub::_AssetMap::see_field_changelogs;
}

impl<T: HasChangelog> HasChangelog for sol::AssetMap<T>
where
	<T as HasVersion<v20100>>::HistoricalType: Default,
{
	type if_unspecified = sol::_AssetMap::see_field_changelogs;
	type in_20100 =
		sol::_AssetMap::see_field_changelogs_and_also<sol::_AssetMap::field::usdt::Added>;
}

impl<T: HasChangelog> HasChangelog for arb::AssetMap<T>
where
	<T as HasVersion<v20100>>::HistoricalType: Default,
{
	type if_unspecified = arb::_AssetMap::see_field_changelogs;
	type in_20100 =
		arb::_AssetMap::see_field_changelogs_and_also<arb::_AssetMap::field::usdt::Added>;
}

impl<T: HasChangelog> HasChangelog for btc::AssetMap<T> {
	type if_unspecified = btc::_AssetMap::see_field_changelogs;
}

impl<T: HasChangelog> HasChangelog for dot::AssetMap<T> {
	type if_unspecified = dot::_AssetMap::see_field_changelogs;
}

impl<T: HasChangelog> HasChangelog for tron::AssetMap<T> {
	type if_unspecified = tron::_AssetMap::see_field_changelogs;
}

impl<T: HasChangelog> HasChangelog for bsc::AssetMap<T> {
	type if_unspecified = bsc::_AssetMap::see_field_changelogs;
}

impl<T: HasChangelog> HasChangelog for eth::AssetMap<T>
where
	<T as HasVersion<v20100>>::HistoricalType: Default,
{
	type if_unspecified = eth::_AssetMap::see_field_changelogs;
	type in_20100 =
		eth::_AssetMap::see_field_changelogs_and_also<eth::_AssetMap::field::wbtc::Added>;
}

impl<T: HasChangelog + Default> HasChangelog for any::AssetMap<T>
where
	<T as HasVersion<v20100>>::HistoricalType: Default,
	<T as HasVersion<v20200>>::HistoricalType: Default,
	<T as HasVersion<v20300>>::HistoricalType: Default,
{
	type if_unspecified = any::_AssetMap::see_field_changelogs;
	type in_20200 =
		any::_AssetMap::see_field_changelogs_and_also<any::_AssetMap::field::tron::Added>;
	type in_20300 =
		any::_AssetMap::see_field_changelogs_and_also<any::_AssetMap::field::bsc::Added>;
}

// impl HasChangelog for Asset {
// 	type if_unspecified = IdentityMigration;
// }

// --------- testing ------------

#[cf_proc_macros::generate_module]
pub struct MyS<T> {
	pub a: T, // pub a: u8,
}

impl<T: HasChangelog> HasChangelog for MyS<T> {
	type if_unspecified = _MyS::see_field_changelogs;
}

// Recursive expansion of generate_module macro
// =============================================

/*
#[derive(cf_proc_macros::IntroElim)]
pub struct MyS<T> {
	pub a: T,
}
pub mod _MyS {
	#![allow(nonstandard_style)]
	#![allow(unused)]
	use super::*;
	use cf_utilities::migrations::{basics::*, *};
	pub trait Types {
		type a;
	}
	pub trait HistoricalTypesAt<V: Version> = Types<a: IsHistoricalTypeAt<V>>;
	impl<a> Types for (a,) {
		type a = a;
	}
	#[derive(scale_info::TypeInfo)]
	#[derive_where::derive_where(Debug;
	Ty::a: sp_std::fmt::Debug)]
	#[scale_info(skip_type_params(Ty))]
	#[derive(cf_proc_macros::IntroElim)]
	pub struct Struct<T, Ty: Types> {
		pub a: Ty::a,
		_phantom: core::marker::PhantomData<(T,)>,
	}
	#[cfg(any(test, all(feature = "proptest", feature = "std")))]
	impl<T, Ty: Types> proptest::arbitrary::Arbitrary for Struct<T, Ty>
	where
		Ty: 'static,
		T: 'static,
		Ty::a: proptest::arbitrary::Arbitrary + 'static,
	{
		type Parameters = ();
		type Strategy = proptest::strategy::BoxedStrategy<Self>;
		fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
			use proptest::{
				arbitrary::any,
				strategy::{Just, Strategy},
			};
			(Just(()), any::<Ty::a>())
				.prop_map(|(_, a)| Struct::<T, Ty>::intro(a, Default::default()))
				.boxed()
		}
	}
	impl<T, Ty: Types> IsHistoricalType for Struct<T, Ty>
	where
		Ty::a: IsHistoricalType,
		MyS<T>: HasChangelog,
	{
		type GetCurrentType = MyS<T>;
	}
	type UserStruct<T>
		= MyS<T>
	where
		T: HasGenericVariant;
	pub type GenericStruct<T>
		= Struct<T, (GetGenericVariant<T>,)>
	where
		T: HasGenericVariant;
	impl<T> HasGenericVariant for UserStruct<T>
	where
		GenericStruct<T>: IsHistoricalType,
		T: HasGenericVariant,
	{
		type GenericType = GenericStruct<T>;
		type MigrationFromGeneric = GlobalMigrationFromGeneric;
	}
	impl<T> Migration<UserStruct<T>, vCurrent> for GlobalMigrationFromGeneric
	where
		GenericStruct<T>: IsHistoricalType,
		T: HasGenericVariant,
	{
		type From = GenericStruct<T>;
		fn forwards(x: GenericStruct<T>) -> UserStruct<T> {
			MyS {
				a: < <T as HasGenericVariant> ::MigrationFromGeneric as Migration<T,vCurrent> > ::forwards(x.a),
			}
		}
		fn backwards(x: UserStruct<T>) -> GenericStruct<T> {
			Struct::intro(< <T as HasGenericVariant> ::MigrationFromGeneric as Migration<T,vCurrent> > ::backwards(x.a),Default::default(),)
		}
	}
	pub type see_field_changelogs = see_field_changelogs_and_also<()>;
	pub struct see_field_changelogs_and_also<M>(M);

	pub trait FieldCustomMigration<To: HistoricalTypesAt<V>, V: Version> {
		type a: MaybeMigration<To::a, V> = DefaultMigration;
	}
	impl<To: HistoricalTypesAt<V>, V: Version> FieldCustomMigration<To, V> for () {}
	impl<
			M1: FieldCustomMigration<To, V>,
			M2: FieldCustomMigration<To, V>,
			To: HistoricalTypesAt<V>,
			V: Version,
		> FieldCustomMigration<To, V> for (M1, M2)
	{
		type a = (M1::a, M2::a);
	}
	mod field_migrations {
		use super::*;
		type a<To: HistoricalTypesAt<V>, V: Version, M: FieldCustomMigration<To, V>> =
			<M::a as MaybeMigration<To::a, V>>::GetWithDefault<
				GetMigrationToHistoricalType<To::a, V>,
			>;
		pub type TyFrom<To: HistoricalTypesAt<V>, V: Version, M: FieldCustomMigration<To, V>> =
			(<field_migrations::a<To, V, M> as Migration<To::a, V>>::From,);
		pub enum StructForwardsError<
			To: HistoricalTypesAt<V>,
			V: Version,
			M: FieldCustomMigration<To, V>,
		> {
			a(<a<To, V, M> as Migration<To::a, V>>::ForwardsError),
			_phantom(cf_utilities::never::Never, core::marker::PhantomData<(To, V, M)>),
		}
		pub enum StructBackwardsError<
			To: HistoricalTypesAt<V>,
			V: Version,
			M: FieldCustomMigration<To, V>,
		> {
			a(<a<To, V, M> as Migration<To::a, V>>::BackwardsError),
			_phantom(cf_utilities::never::Never, core::marker::PhantomData<(To, V, M)>),
		}
		pub type StructVariant<Target: Types, T> = Struct<T, Target>;
		impl<To: HistoricalTypesAt<V>, V: Version, M: FieldCustomMigration<To, V>, T>
			Migration<Struct<T, To>, V> for see_field_changelogs_and_also<M>
		where
			StructVariant<TyFrom<To, V, M>, T>: IsHistoricalType,
			T: HasChangelog,
		{
			type ForwardsError = StructForwardsError<To, V, M>;
			type BackwardsError = StructBackwardsError<To, V, M>;
			type From = StructVariant<TyFrom<To, V, M>, T>;
			fn forwards(x: StructVariant<TyFrom<To, V, M>, T>) -> StructVariant<To, T> {
				Struct::intro(a::<To, V, M>::forwards(x.a), Default::default())
			}
			fn backwards(x: StructVariant<To, T>) -> StructVariant<TyFrom<To, V, M>, T> {
				Struct::intro(a::<To, V, M>::backwards(x.a), Default::default())
			}
			fn try_forwards<E>(x: Self::From) -> Result<StructVariant<To, T>, E> {
				todo!()
			}
		}
	}
	pub mod field {
		pub mod a {
			use super::super::{
				FieldCustomMigration, HistoricalTypesAt, NewFieldWithDefault,
				OverrideMigrationWith, Version,
			};
			#[derive(Debug)]
			pub struct Added;

			impl<V: Version, TargetFieldsTypes: HistoricalTypesAt<V, a: Default>>
				FieldCustomMigration<TargetFieldsTypes, V> for Added
			{
				type a = OverrideMigrationWith<NewFieldWithDefault>;
			}
		}
	}
}



 */

pub trait T1 {
	type XY;
	type GGG;
	type HHH;
	type Bla;
	type XXX;
	type YYY;
	type FFF;
	type EEE;
	type AAA;
	type BBB;
	type CCC;
	type ZZZ;
}

mod enum1 {
	use super::T1;

	cf_utilities::generate_module! {
		pub enum MyTestValuesWithoutChangelog<T: T1> {
			Variant1(_0: T::XY),
			VariantNone{_0: u8,_1: u16, _2: u8,},
			Variant3(_0: T::XY),
			// Variant5 {
			// 	myfield: u8,
			// 	// field2: (T::XY, T::XY),
			// },
		}
		mod _MyTestValuesWithoutChangelog { #![migrations] }
	}
}

mod enum2 {
	use super::T1;
	cf_utilities::generate_module! {
	pub enum MyTestValuesWithoutChangelog2<T: T1> {
		Variant1(_0: T::XY),
		VariantNone{_0: u8,_1: u16, _2: u8,},
		// Variant3(_0: T::XY),
		Variant5 {
			myfield: u8,
			// field2: (T::XY, T::XY),
		},
	}
		mod _MyTestValuesWithoutChangelog2 { #![migrations] }
	}
}

mod enum3 {
	use super::T1;
	cf_utilities::generate_module! {
	pub enum MyTestValuesWithoutChangelog3<T: T1> {
		Variant1(_0: T::XY),
		VariantNone{_0: u8,_1: u16, _2: u8,},
		// Variant3(_0: T::XY),
		Variant5 {
			myfield: u8,
			// field2: (T::XY, T::XY),
		},
	}
		mod _MyTestValuesWithoutChangelog3 { #![migrations] }
	}
}

mod enum4 {
	use super::{HasChangelog, T1};
	cf_utilities::generate_module! {
	pub enum MyTestValues<T: T1> {
		Variant1(_0: T::XY),
		// Variant2(_0: u8),
		Variant2{_0: u8, _1: u16, _2: u8, _3: u8, _4: u8, _5: u8, _6: u16, _7: u8, },
		// Variant2(_0: u8, _1: u16),
		Variant3 {_0: T::XY, _1: T::XY,},
		Variant4 {
			// myfield: u8,
			field2: T::XY,
			field3: T::XY,
			field4: T::XY,
			field5: T::XY,
		},
		Variant5 (
			// myfield: u8,
			field2: u16,
			field3: u8,
			field4: u8,
			field5: u8,
			field6: u8,
			field7: u8,
			field8: u8,
			field9: u8,
			field10: u8,
			field11: u8,
			field12: u8,
			field13: u8
		),
		Variant6 {
			myfield: u8,
			field2: u16,
			field3: T::XY,
			field4: T::XY,
			field5: T::XY,
		},
	}
		mod _MyTestValues { #![migrations] }
	}

	impl<T: T1 + HasChangelog> HasChangelog for MyTestValues<T>
	where
		T::XY: HasChangelog,
	{
		type if_unspecified = _MyTestValues::see_variant_changelogs;
	}
}

trait Good {
	type Bad;
}

cf_proc_macros::better_modules! {
	mod (A: T1) {
		type MyType = A::XY;
		type Bla = u16;
		struct ThisIsS {
			value: A::XY,
		}
		mod (B: Clone) where (B: Clone) {
			struct InnerWithBoth {
				a: A,
				b: B,
			}
			type MyVal = InnerWithBoth;
		}
		struct Without {
			value: bool,
		}
		impl Good for ThisIsS {
			type Bad = u8;
		}
	}
}

// cf_proc_macros::better_modules! {
// 	trait Test<X> {
// 		fn myfun(x: X) -> X;
// 	}
// 	mod (A) {
// 		struct S1 {
// 			a: A
// 		}
// 	}
// 	mod (B) {
// 		impl Test<S1> for bool {
// 			fn myfun(x: S1) -> S1 {
// 				x
// 			}
// 		}
// 	}
// }

type X = Bla;

// cf_proc_macros::better_modules! {
// 	mod (A: T1) {
// 		struct ThisIsS {
// 			value: A::XY,
// 		}
// 		impl std::fmt::Debug for ThisIsS {

// 		}
// 	}
// }

pub fn mytset() {
	macro_rules! dothis {
		($($x:ident),*) => {
			$(let $x = 0;)*
		};
	}
	cf_utilities::comma_separated_identifiers_for! {dothis; u8, u32}
}
/*
 */
