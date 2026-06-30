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
	v20100, v20200, HasChangelog,
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
{
	type if_unspecified = any::_AssetMap::see_field_changelogs;
	type in_20200 =
		any::_AssetMap::see_field_changelogs_and_also<any::_AssetMap::field::tron::Added>;
}

// impl HasChangelog for Asset {
// 	type if_unspecified = IdentityMigration;
// }

// --------- testing ------------

#[cf_proc_macros::generate_module]
pub struct MyS {
	// pub a: u8,
}

impl HasChangelog for MyS {
	type if_unspecified = _MyS::see_field_changelogs;
}

pub trait T1 {
	type XY;
}

// cf_utilities::generate_module! {
// pub enum MyTestValues<T: T1> {
// 	Variant1(T::XY),
// 	Variant2(u8,u16),
// 	Variant3(T::XY),
// 	Variant4 {
// 		myfield: u8,
// 		field2: (T::XY, T::XY),
// 	},
// }
// 	mod _MyTestValues { #![migrations] }
// }

// ///////////////////////////////////////////


// Recursive expansion of generate_module! macro
// ==============================================

#[derive(cf_proc_macros::EnumElim)]
pub enum MyTestValues<T: T1> {
	Variant1(T::XY),
	Variant2(u8, u16),
	Variant3(T::XY),
	Variant4 { myfield: u8, field2: (T::XY, T::XY) },
}
pub mod _MyTestValues {
	#![allow(nonstandard_style)]
	#![allow(unused)]
	use super::*;
	use cf_utilities::migrations::{basics::*, *};
	pub trait Types {
		type Variant1;
		type Variant2;
		type Variant3;
		type Variant4;
	}
	pub trait HistoricalTypesAt<V: Version> = Types<
		Variant1: IsHistoricalTypeAt<V>,
		Variant2: IsHistoricalTypeAt<V>,
		Variant3: IsHistoricalTypeAt<V>,
		Variant4: IsHistoricalTypeAt<V>,
	>;
	impl<Variant1, Variant2, Variant3, Variant4> Types for (Variant1, Variant2, Variant3, Variant4) {
		type Variant1 = Variant1;
		type Variant2 = Variant2;
		type Variant3 = Variant3;
		type Variant4 = Variant4;
	}
	#[derive_where::derive_where(Debug;
    Ty::Variant1: sp_std::fmt::Debug,Ty::Variant2: sp_std::fmt::Debug,Ty::Variant3: sp_std::fmt::Debug,Ty::Variant4: sp_std::fmt::Debug)]
	pub enum Enum<T: T1, Ty: Types> {
		Variant1(Ty::Variant1),
		Variant2(Ty::Variant2),
		Variant3(Ty::Variant3),
		Variant4(Ty::Variant4),
		_phantom(cf_utilities::never::Never, core::marker::PhantomData<(T, Ty)>),
	}
	impl<T: T1, Ty: Types> IsHistoricalType for Enum<T, Ty>
	where
		Ty::Variant1: IsHistoricalType,
		Ty::Variant2: IsHistoricalType,
		Ty::Variant3: IsHistoricalType,
		Ty::Variant4: IsHistoricalType,
		MyTestValues<T>: HasChangelog,
	{
		type GetCurrentType = MyTestValues<T>;
	}
	impl<T: T1, Ty: Types> scale_info::TypeInfo for Enum<T, Ty>
	where
		Ty: 'static,
		T: 'static,
		Ty::Variant1: scale_info::TypeInfo + 'static,
		Ty::Variant2: scale_info::TypeInfo + 'static,
		Ty::Variant3: scale_info::TypeInfo + 'static,
		Ty::Variant4: scale_info::TypeInfo + 'static,
		Ty::Variant1: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant2: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant3: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant4: cf_utilities::type_introspection::HasTypeIntrospection,
	{
		type Identity = Self;
		fn type_info() -> scale_info::Type {
			let mut _disc: u8 = 0;
			let mut variants = scale_info::build::Variants::new();
			if! <Ty::Variant1 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                let disc = _disc;
                variants = variants.variant("Variant1", |v|{
                    v.index(disc).fields(scale_info::build::Fields::unnamed().field(|f|f.ty:: <Ty::Variant1>()))
                });
                _disc+=1;
            }
			if! <Ty::Variant2 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                let disc = _disc;
                variants = variants.variant("Variant2", |v|{
                    v.index(disc).fields(scale_info::build::Fields::unnamed().field(|f|f.ty:: <Ty::Variant2>()))
                });
                _disc+=1;
            }
			if! <Ty::Variant3 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                let disc = _disc;
                variants = variants.variant("Variant3", |v|{
                    v.index(disc).fields(scale_info::build::Fields::unnamed().field(|f|f.ty:: <Ty::Variant3>()))
                });
                _disc+=1;
            }
			if! <Ty::Variant4 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                let disc = _disc;
                variants = variants.variant("Variant4", |v|{
                    v.index(disc).fields(scale_info::build::Fields::unnamed().field(|f|f.ty:: <Ty::Variant4>()))
                });
                _disc+=1;
            }
			scale_info::Type::builder()
				.path(scale_info::Path::new("Enum", module_path!()))
				.variant(variants)
		}
	}
	impl<T: T1, Ty: Types> codec::Encode for Enum<T, Ty>
	where
		Ty::Variant1: codec::Encode,
		Ty::Variant2: codec::Encode,
		Ty::Variant3: codec::Encode,
		Ty::Variant4: codec::Encode,
		Ty::Variant1: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant2: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant3: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant4: cf_utilities::type_introspection::HasTypeIntrospection,
	{
		fn size_hint(&self) -> usize {
			match self {
				Self::Variant1(val) => 1usize + codec::Encode::size_hint(val),
				Self::Variant2(val) => 1usize + codec::Encode::size_hint(val),
				Self::Variant3(val) => 1usize + codec::Encode::size_hint(val),
				Self::Variant4(val) => 1usize + codec::Encode::size_hint(val),
				Self::_phantom(never, _) => match *never {},
			}
		}
		fn encode_to<__W: codec::Output + ?Sized>(&self, dest: &mut __W) {
			let mut _disc: u8 = 0;
			let Variant1;
			if! <Ty::Variant1 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                Variant1 = _disc;
                _disc+=1;
            }else {
                Variant1 = 0;
            }
			let Variant2;
			if! <Ty::Variant2 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                Variant2 = _disc;
                _disc+=1;
            }else {
                Variant2 = 0;
            }
			let Variant3;
			if! <Ty::Variant3 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                Variant3 = _disc;
                _disc+=1;
            }else {
                Variant3 = 0;
            }
			let Variant4;
			if! <Ty::Variant4 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                Variant4 = _disc;
                _disc+=1;
            }else {
                Variant4 = 0;
            }
			match self {
				Self::Variant1(val) => {
					codec::Encode::encode_to(&Variant1, dest);
					codec::Encode::encode_to(val, dest);
				},
				Self::Variant2(val) => {
					codec::Encode::encode_to(&Variant2, dest);
					codec::Encode::encode_to(val, dest);
				},
				Self::Variant3(val) => {
					codec::Encode::encode_to(&Variant3, dest);
					codec::Encode::encode_to(val, dest);
				},
				Self::Variant4(val) => {
					codec::Encode::encode_to(&Variant4, dest);
					codec::Encode::encode_to(val, dest);
				},
				Self::_phantom(never, _) => match *never {},
			}
		}
	}
	impl<T: T1, Ty: Types> codec::Decode for Enum<T, Ty>
	where
		Ty::Variant1: codec::Decode,
		Ty::Variant2: codec::Decode,
		Ty::Variant3: codec::Decode,
		Ty::Variant4: codec::Decode,
		Ty::Variant1: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant2: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant3: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant4: cf_utilities::type_introspection::HasTypeIntrospection,
	{
		fn decode<__I: codec::Input>(input: &mut __I) -> Result<Self, codec::Error> {
			let idx = <u8 as codec::Decode>::decode(input)?;
			let mut _disc: u8 = 0;
			if! <Ty::Variant1 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                if idx==_disc {
                    return Ok(Self::Variant1(<Ty::Variant1 as codec::Decode> ::decode(input)?));
                }
                _disc+=1;
            }
			if! <Ty::Variant2 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                if idx==_disc {
                    return Ok(Self::Variant2(<Ty::Variant2 as codec::Decode> ::decode(input)?));
                }
                _disc+=1;
            }
			if! <Ty::Variant3 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                if idx==_disc {
                    return Ok(Self::Variant3(<Ty::Variant3 as codec::Decode> ::decode(input)?));
                }
                _disc+=1;
            }
			if! <Ty::Variant4 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                if idx==_disc {
                    return Ok(Self::Variant4(<Ty::Variant4 as codec::Decode> ::decode(input)?));
                }
                _disc+=1;
            }
			Err(codec::Error::from("Invalid variant index"))
		}
	}
	impl<T: T1, Ty: Types> codec::DecodeWithMemTracking for Enum<T, Ty>
	where
		Ty::Variant1: codec::DecodeWithMemTracking,
		Ty::Variant2: codec::DecodeWithMemTracking,
		Ty::Variant3: codec::DecodeWithMemTracking,
		Ty::Variant4: codec::DecodeWithMemTracking,
		Self: codec::Decode,
		Ty::Variant1: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant2: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant3: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant4: cf_utilities::type_introspection::HasTypeIntrospection,
	{
	}
	impl<T: T1, Ty: Types> codec::MaxEncodedLen for Enum<T, Ty>
	where
		Ty::Variant1: codec::MaxEncodedLen,
		Ty::Variant2: codec::MaxEncodedLen,
		Ty::Variant3: codec::MaxEncodedLen,
		Ty::Variant4: codec::MaxEncodedLen,
		Self: codec::Encode,
		Ty::Variant1: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant2: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant3: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant4: cf_utilities::type_introspection::HasTypeIntrospection,
	{
		fn max_encoded_len() -> usize {
			let mut max_variant_size: usize = 0;
			if! <Ty::Variant1 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                let size =  <Ty::Variant1 as codec::MaxEncodedLen> ::max_encoded_len();
                if size>max_variant_size {
                    max_variant_size = size;
                }
            }
			if! <Ty::Variant2 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                let size =  <Ty::Variant2 as codec::MaxEncodedLen> ::max_encoded_len();
                if size>max_variant_size {
                    max_variant_size = size;
                }
            }
			if! <Ty::Variant3 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                let size =  <Ty::Variant3 as codec::MaxEncodedLen> ::max_encoded_len();
                if size>max_variant_size {
                    max_variant_size = size;
                }
            }
			if! <Ty::Variant4 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                let size =  <Ty::Variant4 as codec::MaxEncodedLen> ::max_encoded_len();
                if size>max_variant_size {
                    max_variant_size = size;
                }
            }
			1usize + max_variant_size
		}
	}
	#[cfg(any(test, all(feature = "proptest", feature = "std")))]
	impl<T: T1, Ty: Types> proptest::arbitrary::Arbitrary for Enum<T, Ty>
	where
		Ty: 'static,
		T: 'static,
		Ty::Variant1: proptest::arbitrary::Arbitrary + sp_std::fmt::Debug + 'static,
		Ty::Variant2: proptest::arbitrary::Arbitrary + sp_std::fmt::Debug + 'static,
		Ty::Variant3: proptest::arbitrary::Arbitrary + sp_std::fmt::Debug + 'static,
		Ty::Variant4: proptest::arbitrary::Arbitrary + sp_std::fmt::Debug + 'static,
		Ty::Variant1: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant2: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant3: cf_utilities::type_introspection::HasTypeIntrospection,
		Ty::Variant4: cf_utilities::type_introspection::HasTypeIntrospection,
	{
		type Parameters = ();
		type Strategy = proptest::strategy::BoxedStrategy<Self>;
		fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
			use proptest::strategy::Strategy;
			let mut strategies: Vec<proptest::strategy::BoxedStrategy<Self>> = Vec::new();
			if! <Ty::Variant1 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                strategies.push(proptest::arbitrary::any:: <Ty::Variant1>().prop_map(|val|Enum:: <T,Ty> ::Variant1(val)).boxed());
            }
			if! <Ty::Variant2 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                strategies.push(proptest::arbitrary::any:: <Ty::Variant2>().prop_map(|val|Enum:: <T,Ty> ::Variant2(val)).boxed());
            }
			if! <Ty::Variant3 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                strategies.push(proptest::arbitrary::any:: <Ty::Variant3>().prop_map(|val|Enum:: <T,Ty> ::Variant3(val)).boxed());
            }
			if! <Ty::Variant4 as cf_utilities::type_introspection::HasTypeIntrospection> ::is_empty_type(){
                strategies.push(proptest::arbitrary::any:: <Ty::Variant4>().prop_map(|val|Enum:: <T,Ty> ::Variant4(val)).boxed());
            }
			{
				if !(!strategies.is_empty()) {
					{
						core::panicking::panic_fmt(core::const_format_args!("All variants of Enum are empty types — cannot generate arbitrary values"));
					};
				}
			};
			proptest::strategy::Union::new(strategies).boxed()
		}
	}
	pub type see_variant_changelogs = see_variant_changelogs_and_also<()>;
	pub struct see_variant_changelogs_and_also<M>(M);

	pub trait VariantCustomMigration<To: Types, V: Version> {
		type Variant1: MaybeMigration<To::Variant1, V> = DefaultMigration;
		type Variant2: MaybeMigration<To::Variant2, V> = DefaultMigration;
		type Variant3: MaybeMigration<To::Variant3, V> = DefaultMigration;
		type Variant4: MaybeMigration<To::Variant4, V> = DefaultMigration;
	}
	impl<To: Types, V: Version> VariantCustomMigration<To, V> for () {}
	impl<
			M1: VariantCustomMigration<To, V>,
			M2: VariantCustomMigration<To, V>,
			To: Types,
			V: Version,
		> VariantCustomMigration<To, V> for (M1, M2)
	{
		type Variant1 = (M1::Variant1, M2::Variant1);
		type Variant2 = (M1::Variant2, M2::Variant2);
		type Variant3 = (M1::Variant3, M2::Variant3);
		type Variant4 = (M1::Variant4, M2::Variant4);
	}
	mod variant_migrations {
		use super::*;
		type Variant1<To: Types, V: Version, M: VariantCustomMigration<To, V>>
			= <M::Variant1 as MaybeMigration<To::Variant1, V>>::GetWithDefault<
			GetMigrationToHistoricalType<To::Variant1, V>,
		>
		where
			To: HistoricalTypesAt<V>;
		type Variant2<To: Types, V: Version, M: VariantCustomMigration<To, V>>
			= <M::Variant2 as MaybeMigration<To::Variant2, V>>::GetWithDefault<
			GetMigrationToHistoricalType<To::Variant2, V>,
		>
		where
			To: HistoricalTypesAt<V>;
		type Variant3<To: Types, V: Version, M: VariantCustomMigration<To, V>>
			= <M::Variant3 as MaybeMigration<To::Variant3, V>>::GetWithDefault<
			GetMigrationToHistoricalType<To::Variant3, V>,
		>
		where
			To: HistoricalTypesAt<V>;
		type Variant4<To: Types, V: Version, M: VariantCustomMigration<To, V>>
			= <M::Variant4 as MaybeMigration<To::Variant4, V>>::GetWithDefault<
			GetMigrationToHistoricalType<To::Variant4, V>,
		>
		where
			To: HistoricalTypesAt<V>;
		pub type FromTy<To: Types, V: Version, M: VariantCustomMigration<To, V>>
			= (
			<variant_migrations::Variant1<To, V, M> as Migration<To::Variant1, V>>::From,
			<variant_migrations::Variant2<To, V, M> as Migration<To::Variant2, V>>::From,
			<variant_migrations::Variant3<To, V, M> as Migration<To::Variant3, V>>::From,
			<variant_migrations::Variant4<To, V, M> as Migration<To::Variant4, V>>::From,
		)
		where
			To: HistoricalTypesAt<V>;
		pub enum ForwardsError<To: Types, V: Version, M: VariantCustomMigration<To, V>>
		where
			To: HistoricalTypesAt<V>,
		{
			Variant1(<variant_migrations::Variant1<To,V,M>as Migration<To::Variant1,V> > ::ForwardsError),Variant2(<variant_migrations::Variant2<To,V,M>as Migration<To::Variant2,V> > ::ForwardsError),Variant3(<variant_migrations::Variant3<To,V,M>as Migration<To::Variant3,V> > ::ForwardsError),Variant4(<variant_migrations::Variant4<To,V,M>as Migration<To::Variant4,V> > ::ForwardsError),_phantom(cf_utilities::never::Never,core::marker::PhantomData<(To,V,M,)>)
        }
		pub enum BackwardsError<To: Types, V: Version, M: VariantCustomMigration<To, V>>
		where
			To: HistoricalTypesAt<V>,
		{
			Variant1(<variant_migrations::Variant1<To,V,M>as Migration<To::Variant1,V> > ::BackwardsError),Variant2(<variant_migrations::Variant2<To,V,M>as Migration<To::Variant2,V> > ::BackwardsError),Variant3(<variant_migrations::Variant3<To,V,M>as Migration<To::Variant3,V> > ::BackwardsError),Variant4(<variant_migrations::Variant4<To,V,M>as Migration<To::Variant4,V> > ::BackwardsError),_phantom(cf_utilities::never::Never,core::marker::PhantomData<(To,V,M,)>)
        }
		pub type EnumVariant<Target: Types, T: T1> = Enum<T, Target>;
		impl<To: Types, V: Version, M: VariantCustomMigration<To, V>, T: T1>
			Migration<Enum<T, To>, V> for see_variant_changelogs_and_also<M>
		where
			EnumVariant<FromTy<To, V, M>, T>: IsHistoricalType,
			To: HistoricalTypesAt<V>,
		{
			type From = EnumVariant<FromTy<To, V, M>, T>;
			type ForwardsError = variant_migrations::ForwardsError<To, V, M>;
			type BackwardsError = variant_migrations::BackwardsError<To, V, M>;
			fn forwards<E: From<Self::ForwardsError>>(
				x: EnumVariant<FromTy<To, V, M>, T>,
			) -> Result<EnumVariant<To, T>, E> {
				Ok(match x {
					Enum::Variant1(val) => Enum::Variant1(
						Variant1::<To, V, M>::forwards::<
							<Variant1<To, V, M> as Migration<To::Variant1, V>>::ForwardsError,
						>(val)
						.map_err(variant_migrations::ForwardsError::<To, V, M>::Variant1)
						.map_err(E::from)?,
					),
					Enum::Variant2(val) => Enum::Variant2(
						Variant2::<To, V, M>::forwards::<
							<Variant2<To, V, M> as Migration<To::Variant2, V>>::ForwardsError,
						>(val)
						.map_err(variant_migrations::ForwardsError::<To, V, M>::Variant2)
						.map_err(E::from)?,
					),
					Enum::Variant3(val) => Enum::Variant3(
						Variant3::<To, V, M>::forwards::<
							<Variant3<To, V, M> as Migration<To::Variant3, V>>::ForwardsError,
						>(val)
						.map_err(variant_migrations::ForwardsError::<To, V, M>::Variant3)
						.map_err(E::from)?,
					),
					Enum::Variant4(val) => Enum::Variant4(
						Variant4::<To, V, M>::forwards::<
							<Variant4<To, V, M> as Migration<To::Variant4, V>>::ForwardsError,
						>(val)
						.map_err(variant_migrations::ForwardsError::<To, V, M>::Variant4)
						.map_err(E::from)?,
					),
					Enum::_phantom(never, _) => Enum::_phantom(never, Default::default()),
				})
			}
			fn backwards<E: From<Self::BackwardsError>>(
				x: EnumVariant<To, T>,
			) -> Result<EnumVariant<FromTy<To, V, M>, T>, E> {
				Ok(match x {
					Enum::Variant1(val) => Enum::Variant1(
						Variant1::<To, V, M>::backwards::<
							<Variant1<To, V, M> as Migration<To::Variant1, V>>::BackwardsError,
						>(val)
						.map_err(variant_migrations::BackwardsError::<To, V, M>::Variant1)
						.map_err(E::from)?,
					),
					Enum::Variant2(val) => Enum::Variant2(
						Variant2::<To, V, M>::backwards::<
							<Variant2<To, V, M> as Migration<To::Variant2, V>>::BackwardsError,
						>(val)
						.map_err(variant_migrations::BackwardsError::<To, V, M>::Variant2)
						.map_err(E::from)?,
					),
					Enum::Variant3(val) => Enum::Variant3(
						Variant3::<To, V, M>::backwards::<
							<Variant3<To, V, M> as Migration<To::Variant3, V>>::BackwardsError,
						>(val)
						.map_err(variant_migrations::BackwardsError::<To, V, M>::Variant3)
						.map_err(E::from)?,
					),
					Enum::Variant4(val) => Enum::Variant4(
						Variant4::<To, V, M>::backwards::<
							<Variant4<To, V, M> as Migration<To::Variant4, V>>::BackwardsError,
						>(val)
						.map_err(variant_migrations::BackwardsError::<To, V, M>::Variant4)
						.map_err(E::from)?,
					),
					Enum::_phantom(never, _) => Enum::_phantom(never, Default::default()),
				})
			}
		}
	}
	type RealEnum<T: T1> = MyTestValues<T>;
	pub mod variants {
		use super::*;
		pub mod __impls {
			use super::*;
			pub mod Variant1 {
				use super::*;
				#[derive(cf_proc_macros::IntroElim)]
				pub struct variant_struct<T: T1> {
					pub value: (T::XY,),
					_phantom: core::marker::PhantomData<()>,
				}
				pub mod variant_mod {
					#![allow(nonstandard_style)]
					#![allow(unused)]
					use super::*;
					use cf_utilities::migrations::{basics::*, *};
					pub trait Types {
						type value;
						type _phantom;
					}
					pub trait HistoricalTypesAt<V: Version> =
						Types<value: IsHistoricalTypeAt<V>, _phantom: IsHistoricalTypeAt<V>>;
					impl<value, _phantom> Types for (value, _phantom) {
						type value = value;
						type _phantom = _phantom;
					}
					#[derive_where(Debug;
                    Ty::value: sp_std::fmt::Debug,Ty::_phantom: sp_std::fmt::Debug)]
					#[scale_info(skip_type_params(Ty))]
					pub struct Struct<T: T1, Ty: Types> {
						pub value: Ty::value,
						pub _phantom: Ty::_phantom,
						_phantom2: core::marker::PhantomData<(T,)>,
					}
					#[cfg(any(test, all(feature = "proptest", feature = "std")))]
					impl<T: T1, Ty: Types> proptest::arbitrary::Arbitrary for Struct<T, Ty>
					where
						Ty: 'static,
						T: 'static,
						Ty::value: proptest::arbitrary::Arbitrary + 'static,
						Ty::_phantom: proptest::arbitrary::Arbitrary + 'static,
					{
						type Parameters = ();
						type Strategy = proptest::strategy::BoxedStrategy<Self>;
						fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
							use proptest::{
								arbitrary::any,
								strategy::{Just, Strategy},
							};
							(Just(()), any::<Ty::value>(), any::<Ty::_phantom>())
								.prop_map(|(_, value, _phantom)| {
									Struct::<T, Ty>::intro(value, _phantom, Default::default())
								})
								.boxed()
						}
					}
					impl<T: T1, Ty: Types> IsHistoricalType for Struct<T, Ty>
					where
						Ty::value: IsHistoricalType,
						Ty::_phantom: IsHistoricalType,
						variant_struct<T>: HasChangelog,
					{
						type GetCurrentType = variant_struct<T>;
					}
					type UserStruct<T: T1>
						= variant_struct<T>
					where
						(T::XY,): HasGenericVariant;
					pub type GenericStruct<T: T1>
						= Struct<
						T,
						(
							GetGenericVariant<(T::XY,)>,
							GetGenericVariant<core::marker::PhantomData<()>>,
						),
					>
					where
						(T::XY,): HasGenericVariant;
					pub enum GenericForwardsError<T: T1>
					where
						(T::XY,): HasGenericVariant,
						core::marker::PhantomData<()>: HasGenericVariant,
					{
						value(< <(T::XY,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,),vCurrent> > ::ForwardsError),_phantom(< <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::ForwardsError),_phantom(cf_utilities::never::Never,core::marker::PhantomData<(T,)>)
                    }
					pub enum GenericBackwardsError<T: T1>
					where
						(T::XY,): HasGenericVariant,
						core::marker::PhantomData<()>: HasGenericVariant,
					{
						value(< <(T::XY,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,),vCurrent> > ::BackwardsError),_phantom(< <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::BackwardsError),_phantom(cf_utilities::never::Never,core::marker::PhantomData<(T,)>)
                    }
					impl<T: T1> HasGenericVariant for UserStruct<T>
					where
						GenericStruct<T>: IsHistoricalType,
						(T::XY,): HasGenericVariant,
						core::marker::PhantomData<()>: HasGenericVariant,
					{
						type GenericType = GenericStruct<T>;
						type MigrationFromGeneric = GlobalMigrationFromGeneric;
					}
					impl<T: T1> Migration<UserStruct<T>, vCurrent> for GlobalMigrationFromGeneric
					where
						GenericStruct<T>: IsHistoricalType,
						(T::XY,): HasGenericVariant,
						core::marker::PhantomData<()>: HasGenericVariant,
					{
						type From = GenericStruct<T>;
						type ForwardsError = GenericForwardsError<T>;
						type BackwardsError = GenericBackwardsError<T>;
						fn forwards<E: From<Self::ForwardsError>>(
							x: GenericStruct<T>,
						) -> Result<UserStruct<T>, E> {
							Ok(variant_struct {
                                value: < <(T::XY,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,),vCurrent> > ::forwards:: < < <(T::XY,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,),vCurrent> > ::ForwardsError, >(x.value).map_err(GenericForwardsError:: <T> ::value).map_err(E::from)? ,_phantom: < <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::forwards:: < < <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::ForwardsError, >(x._phantom).map_err(GenericForwardsError:: <T> ::_phantom).map_err(E::from)? ,
                            })
						}
						fn backwards<E: From<Self::BackwardsError>>(
							x: UserStruct<T>,
						) -> Result<GenericStruct<T>, E> {
							Ok(Struct::intro(< <(T::XY,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,),vCurrent> > ::backwards:: < < <(T::XY,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,),vCurrent> > ::BackwardsError, >(x.value).map_err(GenericBackwardsError:: <T> ::value).map_err(E::from)? , < <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::backwards:: < < <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::BackwardsError, >(x._phantom).map_err(GenericBackwardsError:: <T> ::_phantom).map_err(E::from)? ,Default::default(),))
						}
					}
					pub type see_field_changelogs = see_field_changelogs_and_also<()>;
					pub struct see_field_changelogs_and_also<M>(M);

					pub trait FieldCustomMigration<To: Types, V: Version> {
						type value: MaybeMigration<To::value, V> = DefaultMigration;
						type _phantom: MaybeMigration<To::_phantom, V> = DefaultMigration;
					}
					impl<To: Types, V: Version> FieldCustomMigration<To, V> for () {}
					impl<
							M1: FieldCustomMigration<To, V>,
							M2: FieldCustomMigration<To, V>,
							To: Types,
							V: Version,
						> FieldCustomMigration<To, V> for (M1, M2)
					{
						type value = (M1::value, M2::value);
						type _phantom = (M1::_phantom, M2::_phantom);
					}
					mod field_migrations {
						use super::*;
						type value<To: Types, V: Version, M: FieldCustomMigration<To, V>>
							= <M::value as MaybeMigration<To::value, V>>::GetWithDefault<
							GetMigrationToHistoricalType<To::value, V>,
						>
						where
							To: HistoricalTypesAt<V>;
						type _phantom<To: Types, V: Version, M: FieldCustomMigration<To, V>>
							= <M::_phantom as MaybeMigration<To::_phantom, V>>::GetWithDefault<
							GetMigrationToHistoricalType<To::_phantom, V>,
						>
						where
							To: HistoricalTypesAt<V>;
						pub type FromTy<To: Types,V: Version,M: FieldCustomMigration<To,V> >  = (<field_migrations::value<To,V,M>as Migration<To::value,V> > ::From, <field_migrations::_phantom<To,V,M>as Migration<To::_phantom,V> > ::From,)where To: HistoricalTypesAt<V> ;
						pub enum ForwardsError<
							To: Types,
							V: Version,
							M: FieldCustomMigration<To, V>,
						>
						where
							To: HistoricalTypesAt<V>,
						{
							value(
								<field_migrations::value<To, V, M> as Migration<
										To::value,
										V,
									>>::ForwardsError,
							),
							_phantom(
								<field_migrations::_phantom<To, V, M> as Migration<
									To::_phantom,
									V,
								>>::ForwardsError,
							),
							_phantom(
								cf_utilities::never::Never,
								core::marker::PhantomData<(To, V, M)>,
							),
						}
						pub enum BackwardsError<
							To: Types,
							V: Version,
							M: FieldCustomMigration<To, V>,
						>
						where
							To: HistoricalTypesAt<V>,
						{
							value(
								<field_migrations::value<To, V, M> as Migration<
										To::value,
										V,
									>>::BackwardsError,
							),
							_phantom(
								<field_migrations::_phantom<To, V, M> as Migration<
									To::_phantom,
									V,
								>>::BackwardsError,
							),
							_phantom(
								cf_utilities::never::Never,
								core::marker::PhantomData<(To, V, M)>,
							),
						}
						pub type StructVariant<Target: Types, T: T1> = Struct<T, Target>;
						impl<To: Types, V: Version, M: FieldCustomMigration<To, V>, T: T1>
							Migration<Struct<T, To>, V> for see_field_changelogs_and_also<M>
						where
							StructVariant<FromTy<To, V, M>, T>: IsHistoricalType,
							To: HistoricalTypesAt<V>,
						{
							type From = StructVariant<FromTy<To, V, M>, T>;
							type ForwardsError = field_migrations::ForwardsError<To, V, M>;
							type BackwardsError = field_migrations::BackwardsError<To, V, M>;
							fn forwards<E: From<Self::ForwardsError>>(
								x: StructVariant<FromTy<To, V, M>, T>,
							) -> Result<StructVariant<To, T>, E> {
								Ok(Struct::intro(value:: <To,V,M> ::forwards:: < <value<To,V,M>as Migration<To::value,V> > ::ForwardsError>(x.value).map_err(field_migrations::ForwardsError:: <To,V,M> ::value).map_err(E::from)? ,_phantom:: <To,V,M> ::forwards:: < <_phantom<To,V,M>as Migration<To::_phantom,V> > ::ForwardsError>(x._phantom).map_err(field_migrations::ForwardsError:: <To,V,M> ::_phantom).map_err(E::from)? ,Default::default(),))
							}
							fn backwards<E: From<Self::BackwardsError>>(
								x: StructVariant<To, T>,
							) -> Result<StructVariant<FromTy<To, V, M>, T>, E> {
								Ok(Struct::intro(value:: <To,V,M> ::backwards:: < <value<To,V,M>as Migration<To::value,V> > ::BackwardsError>(x.value).map_err(field_migrations::BackwardsError:: <To,V,M> ::value).map_err(E::from)? ,_phantom:: <To,V,M> ::backwards:: < <_phantom<To,V,M>as Migration<To::_phantom,V> > ::BackwardsError>(x._phantom).map_err(field_migrations::BackwardsError:: <To,V,M> ::_phantom).map_err(E::from)? ,Default::default(),))
							}
						}
					}
					pub mod field {
						pub mod value {
							use super::super::{
								FieldCustomMigration, HistoricalTypesAt, NewFieldWithDefault,
								OverrideMigrationWith, Version,
							};
							#[derive(Debug)]
							pub struct Added;

							impl<
									V: Version,
									TargetFieldsTypes: HistoricalTypesAt<V, value: Default>,
								> FieldCustomMigration<TargetFieldsTypes, V> for Added
							{
								type value = OverrideMigrationWith<NewFieldWithDefault>;
							}
						}
						pub mod _phantom {
							use super::super::{
								FieldCustomMigration, HistoricalTypesAt, NewFieldWithDefault,
								OverrideMigrationWith, Version,
							};
							#[derive(Debug)]
							pub struct Added;

							impl<
									V: Version,
									TargetFieldsTypes: HistoricalTypesAt<V, _phantom: Default>,
								> FieldCustomMigration<TargetFieldsTypes, V> for Added
							{
								type _phantom = OverrideMigrationWith<NewFieldWithDefault>;
							}
						}
					}
				}
				impl<T: T1> HasChangelog for variant_struct<T>
				where
					(T::XY,): HasChangelog,
				{
					type if_unspecified = variant_mod::see_field_changelogs;
				}
			}
			pub mod Variant2 {
				use super::*;
				#[derive(cf_proc_macros::IntroElim)]
				pub struct variant_struct<T: T1> {
					pub value: (u8, u16),
					_phantom: core::marker::PhantomData<(T,)>,
				}
				pub mod variant_mod {
					#![allow(nonstandard_style)]
					#![allow(unused)]
					use super::*;
					use cf_utilities::migrations::{basics::*, *};
					pub trait Types {
						type value;
						type _phantom;
					}
					pub trait HistoricalTypesAt<V: Version> =
						Types<value: IsHistoricalTypeAt<V>, _phantom: IsHistoricalTypeAt<V>>;
					impl<value, _phantom> Types for (value, _phantom) {
						type value = value;
						type _phantom = _phantom;
					}
					#[derive_where(Debug;
                    Ty::value: sp_std::fmt::Debug,Ty::_phantom: sp_std::fmt::Debug)]
					#[scale_info(skip_type_params(Ty))]
					pub struct Struct<T: T1, Ty: Types> {
						pub value: Ty::value,
						pub _phantom: Ty::_phantom,
						_phantom2: core::marker::PhantomData<(T,)>,
					}
					#[cfg(any(test, all(feature = "proptest", feature = "std")))]
					impl<T: T1, Ty: Types> proptest::arbitrary::Arbitrary for Struct<T, Ty>
					where
						Ty: 'static,
						T: 'static,
						Ty::value: proptest::arbitrary::Arbitrary + 'static,
						Ty::_phantom: proptest::arbitrary::Arbitrary + 'static,
					{
						type Parameters = ();
						type Strategy = proptest::strategy::BoxedStrategy<Self>;
						fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
							use proptest::{
								arbitrary::any,
								strategy::{Just, Strategy},
							};
							(Just(()), any::<Ty::value>(), any::<Ty::_phantom>())
								.prop_map(|(_, value, _phantom)| {
									Struct::<T, Ty>::intro(value, _phantom, Default::default())
								})
								.boxed()
						}
					}
					impl<T: T1, Ty: Types> IsHistoricalType for Struct<T, Ty>
					where
						Ty::value: IsHistoricalType,
						Ty::_phantom: IsHistoricalType,
						variant_struct<T>: HasChangelog,
					{
						type GetCurrentType = variant_struct<T>;
					}
					type UserStruct<T: T1>
						= variant_struct<T>
					where
						core::marker::PhantomData<(T,)>: HasGenericVariant;
					pub type GenericStruct<T: T1>
						= Struct<
						T,
						(
							GetGenericVariant<(u8, u16)>,
							GetGenericVariant<core::marker::PhantomData<(T,)>>,
						),
					>
					where
						core::marker::PhantomData<(T,)>: HasGenericVariant;
					pub enum GenericForwardsError<T: T1>
					where
						(u8, u16): HasGenericVariant,
						core::marker::PhantomData<(T,)>: HasGenericVariant,
					{
						value(< <(u8,u16,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(u8,u16,),vCurrent> > ::ForwardsError),_phantom(< <core::marker::PhantomData<(T,)>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<(T,)> ,vCurrent> > ::ForwardsError),_phantom(cf_utilities::never::Never,core::marker::PhantomData<(T,)>)
                    }
					pub enum GenericBackwardsError<T: T1>
					where
						(u8, u16): HasGenericVariant,
						core::marker::PhantomData<(T,)>: HasGenericVariant,
					{
						value(< <(u8,u16,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(u8,u16,),vCurrent> > ::BackwardsError),_phantom(< <core::marker::PhantomData<(T,)>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<(T,)> ,vCurrent> > ::BackwardsError),_phantom(cf_utilities::never::Never,core::marker::PhantomData<(T,)>)
                    }
					impl<T: T1> HasGenericVariant for UserStruct<T>
					where
						GenericStruct<T>: IsHistoricalType,
						(u8, u16): HasGenericVariant,
						core::marker::PhantomData<(T,)>: HasGenericVariant,
					{
						type GenericType = GenericStruct<T>;
						type MigrationFromGeneric = GlobalMigrationFromGeneric;
					}
					impl<T: T1> Migration<UserStruct<T>, vCurrent> for GlobalMigrationFromGeneric
					where
						GenericStruct<T>: IsHistoricalType,
						(u8, u16): HasGenericVariant,
						core::marker::PhantomData<(T,)>: HasGenericVariant,
					{
						type From = GenericStruct<T>;
						type ForwardsError = GenericForwardsError<T>;
						type BackwardsError = GenericBackwardsError<T>;
						fn forwards<E: From<Self::ForwardsError>>(
							x: GenericStruct<T>,
						) -> Result<UserStruct<T>, E> {
							Ok(variant_struct {
                                value: < <(u8,u16,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(u8,u16,),vCurrent> > ::forwards:: < < <(u8,u16,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(u8,u16,),vCurrent> > ::ForwardsError, >(x.value).map_err(GenericForwardsError:: <T> ::value).map_err(E::from)? ,_phantom: < <core::marker::PhantomData<(T,)>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<(T,)> ,vCurrent> > ::forwards:: < < <core::marker::PhantomData<(T,)>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<(T,)> ,vCurrent> > ::ForwardsError, >(x._phantom).map_err(GenericForwardsError:: <T> ::_phantom).map_err(E::from)? ,
                            })
						}
						fn backwards<E: From<Self::BackwardsError>>(
							x: UserStruct<T>,
						) -> Result<GenericStruct<T>, E> {
							Ok(Struct::intro(< <(u8,u16,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(u8,u16,),vCurrent> > ::backwards:: < < <(u8,u16,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(u8,u16,),vCurrent> > ::BackwardsError, >(x.value).map_err(GenericBackwardsError:: <T> ::value).map_err(E::from)? , < <core::marker::PhantomData<(T,)>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<(T,)> ,vCurrent> > ::backwards:: < < <core::marker::PhantomData<(T,)>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<(T,)> ,vCurrent> > ::BackwardsError, >(x._phantom).map_err(GenericBackwardsError:: <T> ::_phantom).map_err(E::from)? ,Default::default(),))
						}
					}
					pub type see_field_changelogs = see_field_changelogs_and_also<()>;
					pub struct see_field_changelogs_and_also<M>(M);

					pub trait FieldCustomMigration<To: Types, V: Version> {
						type value: MaybeMigration<To::value, V> = DefaultMigration;
						type _phantom: MaybeMigration<To::_phantom, V> = DefaultMigration;
					}
					impl<To: Types, V: Version> FieldCustomMigration<To, V> for () {}
					impl<
							M1: FieldCustomMigration<To, V>,
							M2: FieldCustomMigration<To, V>,
							To: Types,
							V: Version,
						> FieldCustomMigration<To, V> for (M1, M2)
					{
						type value = (M1::value, M2::value);
						type _phantom = (M1::_phantom, M2::_phantom);
					}
					mod field_migrations {
						use super::*;
						type value<To: Types, V: Version, M: FieldCustomMigration<To, V>>
							= <M::value as MaybeMigration<To::value, V>>::GetWithDefault<
							GetMigrationToHistoricalType<To::value, V>,
						>
						where
							To: HistoricalTypesAt<V>;
						type _phantom<To: Types, V: Version, M: FieldCustomMigration<To, V>>
							= <M::_phantom as MaybeMigration<To::_phantom, V>>::GetWithDefault<
							GetMigrationToHistoricalType<To::_phantom, V>,
						>
						where
							To: HistoricalTypesAt<V>;
						pub type FromTy<To: Types,V: Version,M: FieldCustomMigration<To,V> >  = (<field_migrations::value<To,V,M>as Migration<To::value,V> > ::From, <field_migrations::_phantom<To,V,M>as Migration<To::_phantom,V> > ::From,)where To: HistoricalTypesAt<V> ;
						pub enum ForwardsError<
							To: Types,
							V: Version,
							M: FieldCustomMigration<To, V>,
						>
						where
							To: HistoricalTypesAt<V>,
						{
							value(
								<field_migrations::value<To, V, M> as Migration<
										To::value,
										V,
									>>::ForwardsError,
							),
							_phantom(
								<field_migrations::_phantom<To, V, M> as Migration<
									To::_phantom,
									V,
								>>::ForwardsError,
							),
							_phantom(
								cf_utilities::never::Never,
								core::marker::PhantomData<(To, V, M)>,
							),
						}
						pub enum BackwardsError<
							To: Types,
							V: Version,
							M: FieldCustomMigration<To, V>,
						>
						where
							To: HistoricalTypesAt<V>,
						{
							value(
								<field_migrations::value<To, V, M> as Migration<
										To::value,
										V,
									>>::BackwardsError,
							),
							_phantom(
								<field_migrations::_phantom<To, V, M> as Migration<
									To::_phantom,
									V,
								>>::BackwardsError,
							),
							_phantom(
								cf_utilities::never::Never,
								core::marker::PhantomData<(To, V, M)>,
							),
						}
						pub type StructVariant<Target: Types, T: T1> = Struct<T, Target>;
						impl<To: Types, V: Version, M: FieldCustomMigration<To, V>, T: T1>
							Migration<Struct<T, To>, V> for see_field_changelogs_and_also<M>
						where
							StructVariant<FromTy<To, V, M>, T>: IsHistoricalType,
							To: HistoricalTypesAt<V>,
						{
							type From = StructVariant<FromTy<To, V, M>, T>;
							type ForwardsError = field_migrations::ForwardsError<To, V, M>;
							type BackwardsError = field_migrations::BackwardsError<To, V, M>;
							fn forwards<E: From<Self::ForwardsError>>(
								x: StructVariant<FromTy<To, V, M>, T>,
							) -> Result<StructVariant<To, T>, E> {
								Ok(Struct::intro(value:: <To,V,M> ::forwards:: < <value<To,V,M>as Migration<To::value,V> > ::ForwardsError>(x.value).map_err(field_migrations::ForwardsError:: <To,V,M> ::value).map_err(E::from)? ,_phantom:: <To,V,M> ::forwards:: < <_phantom<To,V,M>as Migration<To::_phantom,V> > ::ForwardsError>(x._phantom).map_err(field_migrations::ForwardsError:: <To,V,M> ::_phantom).map_err(E::from)? ,Default::default(),))
							}
							fn backwards<E: From<Self::BackwardsError>>(
								x: StructVariant<To, T>,
							) -> Result<StructVariant<FromTy<To, V, M>, T>, E> {
								Ok(Struct::intro(value:: <To,V,M> ::backwards:: < <value<To,V,M>as Migration<To::value,V> > ::BackwardsError>(x.value).map_err(field_migrations::BackwardsError:: <To,V,M> ::value).map_err(E::from)? ,_phantom:: <To,V,M> ::backwards:: < <_phantom<To,V,M>as Migration<To::_phantom,V> > ::BackwardsError>(x._phantom).map_err(field_migrations::BackwardsError:: <To,V,M> ::_phantom).map_err(E::from)? ,Default::default(),))
							}
						}
					}
					pub mod field {
						pub mod value {
							use super::super::{
								FieldCustomMigration, HistoricalTypesAt, NewFieldWithDefault,
								OverrideMigrationWith, Version,
							};
							#[derive(Debug)]
							pub struct Added;

							impl<
									V: Version,
									TargetFieldsTypes: HistoricalTypesAt<V, value: Default>,
								> FieldCustomMigration<TargetFieldsTypes, V> for Added
							{
								type value = OverrideMigrationWith<NewFieldWithDefault>;
							}
						}
						pub mod _phantom {
							use super::super::{
								FieldCustomMigration, HistoricalTypesAt, NewFieldWithDefault,
								OverrideMigrationWith, Version,
							};
							#[derive(Debug)]
							pub struct Added;

							impl<
									V: Version,
									TargetFieldsTypes: HistoricalTypesAt<V, _phantom: Default>,
								> FieldCustomMigration<TargetFieldsTypes, V> for Added
							{
								type _phantom = OverrideMigrationWith<NewFieldWithDefault>;
							}
						}
					}
				}
				impl<T: T1> HasChangelog for variant_struct<T>
				where
					(u8, u16): HasChangelog,
				{
					type if_unspecified = variant_mod::see_field_changelogs;
				}
			}
			pub mod Variant3 {
				use super::*;
				#[derive(cf_proc_macros::IntroElim)]
				pub struct variant_struct<T: T1> {
					pub value: (T::XY,),
					_phantom: core::marker::PhantomData<()>,
				}
				pub mod variant_mod {
					#![allow(nonstandard_style)]
					#![allow(unused)]
					use super::*;
					use cf_utilities::migrations::{basics::*, *};
					pub trait Types {
						type value;
						type _phantom;
					}
					pub trait HistoricalTypesAt<V: Version> =
						Types<value: IsHistoricalTypeAt<V>, _phantom: IsHistoricalTypeAt<V>>;
					impl<value, _phantom> Types for (value, _phantom) {
						type value = value;
						type _phantom = _phantom;
					}
					#[derive_where(Debug;
                    Ty::value: sp_std::fmt::Debug,Ty::_phantom: sp_std::fmt::Debug)]
					#[scale_info(skip_type_params(Ty))]
					pub struct Struct<T: T1, Ty: Types> {
						pub value: Ty::value,
						pub _phantom: Ty::_phantom,
						_phantom2: core::marker::PhantomData<(T,)>,
					}
					#[cfg(any(test, all(feature = "proptest", feature = "std")))]
					impl<T: T1, Ty: Types> proptest::arbitrary::Arbitrary for Struct<T, Ty>
					where
						Ty: 'static,
						T: 'static,
						Ty::value: proptest::arbitrary::Arbitrary + 'static,
						Ty::_phantom: proptest::arbitrary::Arbitrary + 'static,
					{
						type Parameters = ();
						type Strategy = proptest::strategy::BoxedStrategy<Self>;
						fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
							use proptest::{
								arbitrary::any,
								strategy::{Just, Strategy},
							};
							(Just(()), any::<Ty::value>(), any::<Ty::_phantom>())
								.prop_map(|(_, value, _phantom)| {
									Struct::<T, Ty>::intro(value, _phantom, Default::default())
								})
								.boxed()
						}
					}
					impl<T: T1, Ty: Types> IsHistoricalType for Struct<T, Ty>
					where
						Ty::value: IsHistoricalType,
						Ty::_phantom: IsHistoricalType,
						variant_struct<T>: HasChangelog,
					{
						type GetCurrentType = variant_struct<T>;
					}
					type UserStruct<T: T1>
						= variant_struct<T>
					where
						(T::XY,): HasGenericVariant;
					pub type GenericStruct<T: T1>
						= Struct<
						T,
						(
							GetGenericVariant<(T::XY,)>,
							GetGenericVariant<core::marker::PhantomData<()>>,
						),
					>
					where
						(T::XY,): HasGenericVariant;
					pub enum GenericForwardsError<T: T1>
					where
						(T::XY,): HasGenericVariant,
						core::marker::PhantomData<()>: HasGenericVariant,
					{
						value(< <(T::XY,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,),vCurrent> > ::ForwardsError),_phantom(< <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::ForwardsError),_phantom(cf_utilities::never::Never,core::marker::PhantomData<(T,)>)
                    }
					pub enum GenericBackwardsError<T: T1>
					where
						(T::XY,): HasGenericVariant,
						core::marker::PhantomData<()>: HasGenericVariant,
					{
						value(< <(T::XY,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,),vCurrent> > ::BackwardsError),_phantom(< <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::BackwardsError),_phantom(cf_utilities::never::Never,core::marker::PhantomData<(T,)>)
                    }
					impl<T: T1> HasGenericVariant for UserStruct<T>
					where
						GenericStruct<T>: IsHistoricalType,
						(T::XY,): HasGenericVariant,
						core::marker::PhantomData<()>: HasGenericVariant,
					{
						type GenericType = GenericStruct<T>;
						type MigrationFromGeneric = GlobalMigrationFromGeneric;
					}
					impl<T: T1> Migration<UserStruct<T>, vCurrent> for GlobalMigrationFromGeneric
					where
						GenericStruct<T>: IsHistoricalType,
						(T::XY,): HasGenericVariant,
						core::marker::PhantomData<()>: HasGenericVariant,
					{
						type From = GenericStruct<T>;
						type ForwardsError = GenericForwardsError<T>;
						type BackwardsError = GenericBackwardsError<T>;
						fn forwards<E: From<Self::ForwardsError>>(
							x: GenericStruct<T>,
						) -> Result<UserStruct<T>, E> {
							Ok(variant_struct {
                                value: < <(T::XY,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,),vCurrent> > ::forwards:: < < <(T::XY,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,),vCurrent> > ::ForwardsError, >(x.value).map_err(GenericForwardsError:: <T> ::value).map_err(E::from)? ,_phantom: < <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::forwards:: < < <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::ForwardsError, >(x._phantom).map_err(GenericForwardsError:: <T> ::_phantom).map_err(E::from)? ,
                            })
						}
						fn backwards<E: From<Self::BackwardsError>>(
							x: UserStruct<T>,
						) -> Result<GenericStruct<T>, E> {
							Ok(Struct::intro(< <(T::XY,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,),vCurrent> > ::backwards:: < < <(T::XY,)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,),vCurrent> > ::BackwardsError, >(x.value).map_err(GenericBackwardsError:: <T> ::value).map_err(E::from)? , < <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::backwards:: < < <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::BackwardsError, >(x._phantom).map_err(GenericBackwardsError:: <T> ::_phantom).map_err(E::from)? ,Default::default(),))
						}
					}
					pub type see_field_changelogs = see_field_changelogs_and_also<()>;
					pub struct see_field_changelogs_and_also<M>(M);

					pub trait FieldCustomMigration<To: Types, V: Version> {
						type value: MaybeMigration<To::value, V> = DefaultMigration;
						type _phantom: MaybeMigration<To::_phantom, V> = DefaultMigration;
					}
					impl<To: Types, V: Version> FieldCustomMigration<To, V> for () {}
					impl<
							M1: FieldCustomMigration<To, V>,
							M2: FieldCustomMigration<To, V>,
							To: Types,
							V: Version,
						> FieldCustomMigration<To, V> for (M1, M2)
					{
						type value = (M1::value, M2::value);
						type _phantom = (M1::_phantom, M2::_phantom);
					}
					mod field_migrations {
						use super::*;
						type value<To: Types, V: Version, M: FieldCustomMigration<To, V>>
							= <M::value as MaybeMigration<To::value, V>>::GetWithDefault<
							GetMigrationToHistoricalType<To::value, V>,
						>
						where
							To: HistoricalTypesAt<V>;
						type _phantom<To: Types, V: Version, M: FieldCustomMigration<To, V>>
							= <M::_phantom as MaybeMigration<To::_phantom, V>>::GetWithDefault<
							GetMigrationToHistoricalType<To::_phantom, V>,
						>
						where
							To: HistoricalTypesAt<V>;
						pub type FromTy<To: Types,V: Version,M: FieldCustomMigration<To,V> >  = (<field_migrations::value<To,V,M>as Migration<To::value,V> > ::From, <field_migrations::_phantom<To,V,M>as Migration<To::_phantom,V> > ::From,)where To: HistoricalTypesAt<V> ;
						pub enum ForwardsError<
							To: Types,
							V: Version,
							M: FieldCustomMigration<To, V>,
						>
						where
							To: HistoricalTypesAt<V>,
						{
							value(
								<field_migrations::value<To, V, M> as Migration<
										To::value,
										V,
									>>::ForwardsError,
							),
							_phantom(
								<field_migrations::_phantom<To, V, M> as Migration<
									To::_phantom,
									V,
								>>::ForwardsError,
							),
							_phantom(
								cf_utilities::never::Never,
								core::marker::PhantomData<(To, V, M)>,
							),
						}
						pub enum BackwardsError<
							To: Types,
							V: Version,
							M: FieldCustomMigration<To, V>,
						>
						where
							To: HistoricalTypesAt<V>,
						{
							value(
								<field_migrations::value<To, V, M> as Migration<
										To::value,
										V,
									>>::BackwardsError,
							),
							_phantom(
								<field_migrations::_phantom<To, V, M> as Migration<
									To::_phantom,
									V,
								>>::BackwardsError,
							),
							_phantom(
								cf_utilities::never::Never,
								core::marker::PhantomData<(To, V, M)>,
							),
						}
						pub type StructVariant<Target: Types, T: T1> = Struct<T, Target>;
						impl<To: Types, V: Version, M: FieldCustomMigration<To, V>, T: T1>
							Migration<Struct<T, To>, V> for see_field_changelogs_and_also<M>
						where
							StructVariant<FromTy<To, V, M>, T>: IsHistoricalType,
							To: HistoricalTypesAt<V>,
						{
							type From = StructVariant<FromTy<To, V, M>, T>;
							type ForwardsError = field_migrations::ForwardsError<To, V, M>;
							type BackwardsError = field_migrations::BackwardsError<To, V, M>;
							fn forwards<E: From<Self::ForwardsError>>(
								x: StructVariant<FromTy<To, V, M>, T>,
							) -> Result<StructVariant<To, T>, E> {
								Ok(Struct::intro(value:: <To,V,M> ::forwards:: < <value<To,V,M>as Migration<To::value,V> > ::ForwardsError>(x.value).map_err(field_migrations::ForwardsError:: <To,V,M> ::value).map_err(E::from)? ,_phantom:: <To,V,M> ::forwards:: < <_phantom<To,V,M>as Migration<To::_phantom,V> > ::ForwardsError>(x._phantom).map_err(field_migrations::ForwardsError:: <To,V,M> ::_phantom).map_err(E::from)? ,Default::default(),))
							}
							fn backwards<E: From<Self::BackwardsError>>(
								x: StructVariant<To, T>,
							) -> Result<StructVariant<FromTy<To, V, M>, T>, E> {
								Ok(Struct::intro(value:: <To,V,M> ::backwards:: < <value<To,V,M>as Migration<To::value,V> > ::BackwardsError>(x.value).map_err(field_migrations::BackwardsError:: <To,V,M> ::value).map_err(E::from)? ,_phantom:: <To,V,M> ::backwards:: < <_phantom<To,V,M>as Migration<To::_phantom,V> > ::BackwardsError>(x._phantom).map_err(field_migrations::BackwardsError:: <To,V,M> ::_phantom).map_err(E::from)? ,Default::default(),))
							}
						}
					}
					pub mod field {
						pub mod value {
							use super::super::{
								FieldCustomMigration, HistoricalTypesAt, NewFieldWithDefault,
								OverrideMigrationWith, Version,
							};
							#[derive(Debug)]
							pub struct Added;

							impl<
									V: Version,
									TargetFieldsTypes: HistoricalTypesAt<V, value: Default>,
								> FieldCustomMigration<TargetFieldsTypes, V> for Added
							{
								type value = OverrideMigrationWith<NewFieldWithDefault>;
							}
						}
						pub mod _phantom {
							use super::super::{
								FieldCustomMigration, HistoricalTypesAt, NewFieldWithDefault,
								OverrideMigrationWith, Version,
							};
							#[derive(Debug)]
							pub struct Added;

							impl<
									V: Version,
									TargetFieldsTypes: HistoricalTypesAt<V, _phantom: Default>,
								> FieldCustomMigration<TargetFieldsTypes, V> for Added
							{
								type _phantom = OverrideMigrationWith<NewFieldWithDefault>;
							}
						}
					}
				}
				impl<T: T1> HasChangelog for variant_struct<T>
				where
					(T::XY,): HasChangelog,
				{
					type if_unspecified = variant_mod::see_field_changelogs;
				}
			}
			pub mod Variant4 {
				use super::*;
				#[derive(cf_proc_macros::IntroElim)]
				pub struct variant_struct<T: T1> {
					pub myfield: u8,
					pub field2: (T::XY, T::XY),
					_phantom: core::marker::PhantomData<()>,
				}
				pub mod variant_mod {
					#![allow(nonstandard_style)]
					#![allow(unused)]
					use super::*;
					use cf_utilities::migrations::{basics::*, *};
					pub trait Types {
						type myfield;
						type field2;
						type _phantom;
					}
					pub trait HistoricalTypesAt<V: Version> = Types<
						myfield: IsHistoricalTypeAt<V>,
						field2: IsHistoricalTypeAt<V>,
						_phantom: IsHistoricalTypeAt<V>,
					>;
					impl<myfield, field2, _phantom> Types for (myfield, field2, _phantom) {
						type myfield = myfield;
						type field2 = field2;
						type _phantom = _phantom;
					}
					#[derive_where(Debug;
                    Ty::myfield: sp_std::fmt::Debug,Ty::field2: sp_std::fmt::Debug,Ty::_phantom: sp_std::fmt::Debug)]
					#[scale_info(skip_type_params(Ty))]
					pub struct Struct<T: T1, Ty: Types> {
						pub myfield: Ty::myfield,
						pub field2: Ty::field2,
						pub _phantom: Ty::_phantom,
						_phantom2: core::marker::PhantomData<(T,)>,
					}
					#[cfg(any(test, all(feature = "proptest", feature = "std")))]
					impl<T: T1, Ty: Types> proptest::arbitrary::Arbitrary for Struct<T, Ty>
					where
						Ty: 'static,
						T: 'static,
						Ty::myfield: proptest::arbitrary::Arbitrary + 'static,
						Ty::field2: proptest::arbitrary::Arbitrary + 'static,
						Ty::_phantom: proptest::arbitrary::Arbitrary + 'static,
					{
						type Parameters = ();
						type Strategy = proptest::strategy::BoxedStrategy<Self>;
						fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
							use proptest::{
								arbitrary::any,
								strategy::{Just, Strategy},
							};
							(
								Just(()),
								any::<Ty::myfield>(),
								any::<Ty::field2>(),
								any::<Ty::_phantom>(),
							)
								.prop_map(|(_, myfield, field2, _phantom)| {
									Struct::<T, Ty>::intro(
										myfield,
										field2,
										_phantom,
										Default::default(),
									)
								})
								.boxed()
						}
					}
					impl<T: T1, Ty: Types> IsHistoricalType for Struct<T, Ty>
					where
						Ty::myfield: IsHistoricalType,
						Ty::field2: IsHistoricalType,
						Ty::_phantom: IsHistoricalType,
						variant_struct<T>: HasChangelog,
					{
						type GetCurrentType = variant_struct<T>;
					}
					type UserStruct<T: T1>
						= variant_struct<T>
					where
						(T::XY, T::XY): HasGenericVariant;
					pub type GenericStruct<T: T1>
						= Struct<
						T,
						(
							GetGenericVariant<u8>,
							GetGenericVariant<(T::XY, T::XY)>,
							GetGenericVariant<core::marker::PhantomData<()>>,
						),
					>
					where
						(T::XY, T::XY): HasGenericVariant;
					pub enum GenericForwardsError<T: T1>
					where
						u8: HasGenericVariant,
						(T::XY, T::XY): HasGenericVariant,
						core::marker::PhantomData<()>: HasGenericVariant,
					{
						myfield(< <u8 as HasGenericVariant> ::MigrationFromGeneric as Migration<u8,vCurrent> > ::ForwardsError),field2(< <(T::XY,T::XY)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,T::XY),vCurrent> > ::ForwardsError),_phantom(< <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::ForwardsError),_phantom(cf_utilities::never::Never,core::marker::PhantomData<(T,)>)
                    }
					pub enum GenericBackwardsError<T: T1>
					where
						u8: HasGenericVariant,
						(T::XY, T::XY): HasGenericVariant,
						core::marker::PhantomData<()>: HasGenericVariant,
					{
						myfield(< <u8 as HasGenericVariant> ::MigrationFromGeneric as Migration<u8,vCurrent> > ::BackwardsError),field2(< <(T::XY,T::XY)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,T::XY),vCurrent> > ::BackwardsError),_phantom(< <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::BackwardsError),_phantom(cf_utilities::never::Never,core::marker::PhantomData<(T,)>)
                    }
					impl<T: T1> HasGenericVariant for UserStruct<T>
					where
						GenericStruct<T>: IsHistoricalType,
						u8: HasGenericVariant,
						(T::XY, T::XY): HasGenericVariant,
						core::marker::PhantomData<()>: HasGenericVariant,
					{
						type GenericType = GenericStruct<T>;
						type MigrationFromGeneric = GlobalMigrationFromGeneric;
					}
					impl<T: T1> Migration<UserStruct<T>, vCurrent> for GlobalMigrationFromGeneric
					where
						GenericStruct<T>: IsHistoricalType,
						u8: HasGenericVariant,
						(T::XY, T::XY): HasGenericVariant,
						core::marker::PhantomData<()>: HasGenericVariant,
					{
						type From = GenericStruct<T>;
						type ForwardsError = GenericForwardsError<T>;
						type BackwardsError = GenericBackwardsError<T>;
						fn forwards<E: From<Self::ForwardsError>>(
							x: GenericStruct<T>,
						) -> Result<UserStruct<T>, E> {
							Ok(variant_struct {
                                myfield: < <u8 as HasGenericVariant> ::MigrationFromGeneric as Migration<u8,vCurrent> > ::forwards:: < < <u8 as HasGenericVariant> ::MigrationFromGeneric as Migration<u8,vCurrent> > ::ForwardsError, >(x.myfield).map_err(GenericForwardsError:: <T> ::myfield).map_err(E::from)? ,field2: < <(T::XY,T::XY)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,T::XY),vCurrent> > ::forwards:: < < <(T::XY,T::XY)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,T::XY),vCurrent> > ::ForwardsError, >(x.field2).map_err(GenericForwardsError:: <T> ::field2).map_err(E::from)? ,_phantom: < <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::forwards:: < < <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::ForwardsError, >(x._phantom).map_err(GenericForwardsError:: <T> ::_phantom).map_err(E::from)? ,
                            })
						}
						fn backwards<E: From<Self::BackwardsError>>(
							x: UserStruct<T>,
						) -> Result<GenericStruct<T>, E> {
							Ok(Struct::intro(< <u8 as HasGenericVariant> ::MigrationFromGeneric as Migration<u8,vCurrent> > ::backwards:: < < <u8 as HasGenericVariant> ::MigrationFromGeneric as Migration<u8,vCurrent> > ::BackwardsError, >(x.myfield).map_err(GenericBackwardsError:: <T> ::myfield).map_err(E::from)? , < <(T::XY,T::XY)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,T::XY),vCurrent> > ::backwards:: < < <(T::XY,T::XY)as HasGenericVariant> ::MigrationFromGeneric as Migration<(T::XY,T::XY),vCurrent> > ::BackwardsError, >(x.field2).map_err(GenericBackwardsError:: <T> ::field2).map_err(E::from)? , < <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::backwards:: < < <core::marker::PhantomData<()>as HasGenericVariant> ::MigrationFromGeneric as Migration<core::marker::PhantomData<()> ,vCurrent> > ::BackwardsError, >(x._phantom).map_err(GenericBackwardsError:: <T> ::_phantom).map_err(E::from)? ,Default::default(),))
						}
					}
					pub type see_field_changelogs = see_field_changelogs_and_also<()>;
					pub struct see_field_changelogs_and_also<M>(M);

					pub trait FieldCustomMigration<To: Types, V: Version> {
						type myfield: MaybeMigration<To::myfield, V> = DefaultMigration;
						type field2: MaybeMigration<To::field2, V> = DefaultMigration;
						type _phantom: MaybeMigration<To::_phantom, V> = DefaultMigration;
					}
					impl<To: Types, V: Version> FieldCustomMigration<To, V> for () {}
					impl<
							M1: FieldCustomMigration<To, V>,
							M2: FieldCustomMigration<To, V>,
							To: Types,
							V: Version,
						> FieldCustomMigration<To, V> for (M1, M2)
					{
						type myfield = (M1::myfield, M2::myfield);
						type field2 = (M1::field2, M2::field2);
						type _phantom = (M1::_phantom, M2::_phantom);
					}
					mod field_migrations {
						use super::*;
						type myfield<To: Types, V: Version, M: FieldCustomMigration<To, V>>
							= <M::myfield as MaybeMigration<To::myfield, V>>::GetWithDefault<
							GetMigrationToHistoricalType<To::myfield, V>,
						>
						where
							To: HistoricalTypesAt<V>;
						type field2<To: Types, V: Version, M: FieldCustomMigration<To, V>>
							= <M::field2 as MaybeMigration<To::field2, V>>::GetWithDefault<
							GetMigrationToHistoricalType<To::field2, V>,
						>
						where
							To: HistoricalTypesAt<V>;
						type _phantom<To: Types, V: Version, M: FieldCustomMigration<To, V>>
							= <M::_phantom as MaybeMigration<To::_phantom, V>>::GetWithDefault<
							GetMigrationToHistoricalType<To::_phantom, V>,
						>
						where
							To: HistoricalTypesAt<V>;
						pub type FromTy<To: Types,V: Version,M: FieldCustomMigration<To,V> >  = (<field_migrations::myfield<To,V,M>as Migration<To::myfield,V> > ::From, <field_migrations::field2<To,V,M>as Migration<To::field2,V> > ::From, <field_migrations::_phantom<To,V,M>as Migration<To::_phantom,V> > ::From,)where To: HistoricalTypesAt<V> ;
						pub enum ForwardsError<
							To: Types,
							V: Version,
							M: FieldCustomMigration<To, V>,
						>
						where
							To: HistoricalTypesAt<V>,
						{
							myfield(
								<field_migrations::myfield<To, V, M> as Migration<
									To::myfield,
									V,
								>>::ForwardsError,
							),
							field2(
								<field_migrations::field2<To, V, M> as Migration<
										To::field2,
										V,
									>>::ForwardsError,
							),
							_phantom(
								<field_migrations::_phantom<To, V, M> as Migration<
									To::_phantom,
									V,
								>>::ForwardsError,
							),
							_phantom(
								cf_utilities::never::Never,
								core::marker::PhantomData<(To, V, M)>,
							),
						}
						pub enum BackwardsError<
							To: Types,
							V: Version,
							M: FieldCustomMigration<To, V>,
						>
						where
							To: HistoricalTypesAt<V>,
						{
							myfield(
								<field_migrations::myfield<To, V, M> as Migration<
									To::myfield,
									V,
								>>::BackwardsError,
							),
							field2(
								<field_migrations::field2<To, V, M> as Migration<
										To::field2,
										V,
									>>::BackwardsError,
							),
							_phantom(
								<field_migrations::_phantom<To, V, M> as Migration<
									To::_phantom,
									V,
								>>::BackwardsError,
							),
							_phantom(
								cf_utilities::never::Never,
								core::marker::PhantomData<(To, V, M)>,
							),
						}
						pub type StructVariant<Target: Types, T: T1> = Struct<T, Target>;
						impl<To: Types, V: Version, M: FieldCustomMigration<To, V>, T: T1>
							Migration<Struct<T, To>, V> for see_field_changelogs_and_also<M>
						where
							StructVariant<FromTy<To, V, M>, T>: IsHistoricalType,
							To: HistoricalTypesAt<V>,
						{
							type From = StructVariant<FromTy<To, V, M>, T>;
							type ForwardsError = field_migrations::ForwardsError<To, V, M>;
							type BackwardsError = field_migrations::BackwardsError<To, V, M>;
							fn forwards<E: From<Self::ForwardsError>>(
								x: StructVariant<FromTy<To, V, M>, T>,
							) -> Result<StructVariant<To, T>, E> {
								Ok(Struct::intro(myfield:: <To,V,M> ::forwards:: < <myfield<To,V,M>as Migration<To::myfield,V> > ::ForwardsError>(x.myfield).map_err(field_migrations::ForwardsError:: <To,V,M> ::myfield).map_err(E::from)? ,field2:: <To,V,M> ::forwards:: < <field2<To,V,M>as Migration<To::field2,V> > ::ForwardsError>(x.field2).map_err(field_migrations::ForwardsError:: <To,V,M> ::field2).map_err(E::from)? ,_phantom:: <To,V,M> ::forwards:: < <_phantom<To,V,M>as Migration<To::_phantom,V> > ::ForwardsError>(x._phantom).map_err(field_migrations::ForwardsError:: <To,V,M> ::_phantom).map_err(E::from)? ,Default::default(),))
							}
							fn backwards<E: From<Self::BackwardsError>>(
								x: StructVariant<To, T>,
							) -> Result<StructVariant<FromTy<To, V, M>, T>, E> {
								Ok(Struct::intro(myfield:: <To,V,M> ::backwards:: < <myfield<To,V,M>as Migration<To::myfield,V> > ::BackwardsError>(x.myfield).map_err(field_migrations::BackwardsError:: <To,V,M> ::myfield).map_err(E::from)? ,field2:: <To,V,M> ::backwards:: < <field2<To,V,M>as Migration<To::field2,V> > ::BackwardsError>(x.field2).map_err(field_migrations::BackwardsError:: <To,V,M> ::field2).map_err(E::from)? ,_phantom:: <To,V,M> ::backwards:: < <_phantom<To,V,M>as Migration<To::_phantom,V> > ::BackwardsError>(x._phantom).map_err(field_migrations::BackwardsError:: <To,V,M> ::_phantom).map_err(E::from)? ,Default::default(),))
							}
						}
					}
					pub mod field {
						pub mod myfield {
							use super::super::{
								FieldCustomMigration, HistoricalTypesAt, NewFieldWithDefault,
								OverrideMigrationWith, Version,
							};
							#[derive(Debug)]
							pub struct Added;

							impl<
									V: Version,
									TargetFieldsTypes: HistoricalTypesAt<V, myfield: Default>,
								> FieldCustomMigration<TargetFieldsTypes, V> for Added
							{
								type myfield = OverrideMigrationWith<NewFieldWithDefault>;
							}
						}
						pub mod field2 {
							use super::super::{
								FieldCustomMigration, HistoricalTypesAt, NewFieldWithDefault,
								OverrideMigrationWith, Version,
							};
							#[derive(Debug)]
							pub struct Added;

							impl<
									V: Version,
									TargetFieldsTypes: HistoricalTypesAt<V, field2: Default>,
								> FieldCustomMigration<TargetFieldsTypes, V> for Added
							{
								type field2 = OverrideMigrationWith<NewFieldWithDefault>;
							}
						}
						pub mod _phantom {
							use super::super::{
								FieldCustomMigration, HistoricalTypesAt, NewFieldWithDefault,
								OverrideMigrationWith, Version,
							};
							#[derive(Debug)]
							pub struct Added;

							impl<
									V: Version,
									TargetFieldsTypes: HistoricalTypesAt<V, _phantom: Default>,
								> FieldCustomMigration<TargetFieldsTypes, V> for Added
							{
								type _phantom = OverrideMigrationWith<NewFieldWithDefault>;
							}
						}
					}
				}
				impl<T: T1> HasChangelog for variant_struct<T>
				where
					u8: HasChangelog,
					(T::XY, T::XY): HasChangelog,
				{
					type if_unspecified = variant_mod::see_field_changelogs;
				}
			}
		}
		pub type Variant1<T: T1> = __impls::Variant1::variant_struct<T>;
		impl<T: T1> Into<RealEnum<T>> for Variant1<T> {
			fn into(self) -> RealEnum<T> {
				{
					#[allow(unused)]
					let (_tv0,) = (self.value);
					MyTestValues::Variant1(_tv0)
				}
			}
		}
		pub type Variant2<T: T1> = __impls::Variant2::variant_struct<T>;
		impl<T: T1> Into<RealEnum<T>> for Variant2<T> {
			fn into(self) -> RealEnum<T> {
				{
					#[allow(unused)]
					let (_tv0, _tv1) = (self.value);
					MyTestValues::Variant2(_tv0, _tv1)
				}
			}
		}
		pub type Variant3<T: T1> = __impls::Variant3::variant_struct<T>;
		impl<T: T1> Into<RealEnum<T>> for Variant3<T> {
			fn into(self) -> RealEnum<T> {
				{
					#[allow(unused)]
					let (_tv0,) = (self.value);
					MyTestValues::Variant3(_tv0)
				}
			}
		}
		pub type Variant4<T: T1> = __impls::Variant4::variant_struct<T>;
		impl<T: T1> Into<RealEnum<T>> for Variant4<T> {
			fn into(self) -> RealEnum<T> {
				MyTestValues::Variant4 { myfield: self.myfield, field2: self.field2 }
			}
		}
	}
	pub struct DefaultTypes<T: T1> {
		_phantom: core::marker::PhantomData<(T,)>,
	}
	impl<T: T1> Types for DefaultTypes<T> {
		type Variant1 = variants::Variant1<T>;
		type Variant2 = variants::Variant2<T>;
		type Variant3 = variants::Variant3<T>;
		type Variant4 = variants::Variant4<T>;
	}
	pub enum GenericForwardsError<T: T1> {
		Variant1(
			<<variants::Variant1<T> as HasGenericVariant>::MigrationFromGeneric as Migration<
				variants::Variant1<T>,
				vCurrent,
			>>::ForwardsError,
		),
		Variant2(
			<<variants::Variant2<T> as HasGenericVariant>::MigrationFromGeneric as Migration<
				variants::Variant2<T>,
				vCurrent,
			>>::ForwardsError,
		),
		Variant3(
			<<variants::Variant3<T> as HasGenericVariant>::MigrationFromGeneric as Migration<
				variants::Variant3<T>,
				vCurrent,
			>>::ForwardsError,
		),
		Variant4(
			<<variants::Variant4<T> as HasGenericVariant>::MigrationFromGeneric as Migration<
				variants::Variant4<T>,
				vCurrent,
			>>::ForwardsError,
		),
		_phantom(cf_utilities::never::Never, core::marker::PhantomData<(T,)>),
	}
	pub enum GenericBackwardsError<T: T1> {
		Variant1(
			<<variants::Variant1<T> as HasGenericVariant>::MigrationFromGeneric as Migration<
				variants::Variant1<T>,
				vCurrent,
			>>::BackwardsError,
		),
		Variant2(
			<<variants::Variant2<T> as HasGenericVariant>::MigrationFromGeneric as Migration<
				variants::Variant2<T>,
				vCurrent,
			>>::BackwardsError,
		),
		Variant3(
			<<variants::Variant3<T> as HasGenericVariant>::MigrationFromGeneric as Migration<
				variants::Variant3<T>,
				vCurrent,
			>>::BackwardsError,
		),
		Variant4(
			<<variants::Variant4<T> as HasGenericVariant>::MigrationFromGeneric as Migration<
				variants::Variant4<T>,
				vCurrent,
			>>::BackwardsError,
		),
		_phantom(cf_utilities::never::Never, core::marker::PhantomData<(T,)>),
	}
	impl<T: T1> HasGenericVariant for RealEnum<T>
	where
		(T::XY,): HasChangelog,
		(T::XY,): HasGenericVariant<GenericType: IsHistoricalType>,
		(u8, u16): HasChangelog,
		(u8, u16): HasGenericVariant<GenericType: IsHistoricalType>,
		(T::XY,): HasChangelog,
		(T::XY,): HasGenericVariant<GenericType: IsHistoricalType>,
		u8: HasChangelog,
		(T::XY, T::XY): HasChangelog,
		u8: HasGenericVariant<GenericType: IsHistoricalType>,
		(T::XY, T::XY): HasGenericVariant<GenericType: IsHistoricalType>,
		Enum<
			T,
			(
				GetGenericVariant<variants::Variant1<T>>,
				GetGenericVariant<variants::Variant2<T>>,
				GetGenericVariant<variants::Variant3<T>>,
				GetGenericVariant<variants::Variant4<T>>,
			),
		>: IsHistoricalType,
	{
		type GenericType = Enum<
			T,
			(
				GetGenericVariant<variants::Variant1<T>>,
				GetGenericVariant<variants::Variant2<T>>,
				GetGenericVariant<variants::Variant3<T>>,
				GetGenericVariant<variants::Variant4<T>>,
			),
		>;
		type MigrationFromGeneric = GlobalMigrationFromGeneric;
	}
	impl<T: T1> Migration<RealEnum<T>, vCurrent> for GlobalMigrationFromGeneric
	where
		(T::XY,): HasChangelog,
		(T::XY,): HasGenericVariant<GenericType: IsHistoricalType>,
		(u8, u16): HasChangelog,
		(u8, u16): HasGenericVariant<GenericType: IsHistoricalType>,
		(T::XY,): HasChangelog,
		(T::XY,): HasGenericVariant<GenericType: IsHistoricalType>,
		u8: HasChangelog,
		(T::XY, T::XY): HasChangelog,
		u8: HasGenericVariant<GenericType: IsHistoricalType>,
		(T::XY, T::XY): HasGenericVariant<GenericType: IsHistoricalType>,
		Enum<
			T,
			(
				GetGenericVariant<variants::Variant1<T>>,
				GetGenericVariant<variants::Variant2<T>>,
				GetGenericVariant<variants::Variant3<T>>,
				GetGenericVariant<variants::Variant4<T>>,
			),
		>: IsHistoricalType,
	{
		type From = Enum<
			T,
			(
				GetGenericVariant<variants::Variant1<T>>,
				GetGenericVariant<variants::Variant2<T>>,
				GetGenericVariant<variants::Variant3<T>>,
				GetGenericVariant<variants::Variant4<T>>,
			),
		>;
		type ForwardsError = GenericForwardsError<T>;
		type BackwardsError = GenericBackwardsError<T>;
		fn forwards<E: From<Self::ForwardsError>>(x: Self::From) -> Result<RealEnum<T>, E> {
			Ok(match x {
                Enum::Variant1(val) => (< <variants::Variant1<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant1<T> ,vCurrent> > ::forwards:: < < <variants::Variant1<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant1<T> ,vCurrent> > ::ForwardsError, >(val).map_err(GenericForwardsError:: <T> ::Variant1).map_err(E::from)?).into(),
                Enum::Variant2(val) => (< <variants::Variant2<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant2<T> ,vCurrent> > ::forwards:: < < <variants::Variant2<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant2<T> ,vCurrent> > ::ForwardsError, >(val).map_err(GenericForwardsError:: <T> ::Variant2).map_err(E::from)?).into(),
                Enum::Variant3(val) => (< <variants::Variant3<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant3<T> ,vCurrent> > ::forwards:: < < <variants::Variant3<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant3<T> ,vCurrent> > ::ForwardsError, >(val).map_err(GenericForwardsError:: <T> ::Variant3).map_err(E::from)?).into(),
                Enum::Variant4(val) => (< <variants::Variant4<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant4<T> ,vCurrent> > ::forwards:: < < <variants::Variant4<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant4<T> ,vCurrent> > ::ForwardsError, >(val).map_err(GenericForwardsError:: <T> ::Variant4).map_err(E::from)?).into(),
                Enum::_phantom(never,_) => match never {},
            })
		}
		fn backwards<E: From<Self::BackwardsError>>(x: RealEnum<T>) -> Result<Self::From, E> {
			x.elim(|x: (T::XY,)|Ok(Enum::Variant1(< <variants::Variant1<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant1<T> ,vCurrent> > ::backwards:: < < <variants::Variant1<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant1<T> ,vCurrent> > ::BackwardsError, >(variants::Variant1:: <T> ::intro({
                let _ = core::marker::PhantomData:: <(T::XY,)> ;
                x
            },Default::default(),)).map_err(GenericBackwardsError:: <T> ::Variant1).map_err(E::from)?)), |x: (u8,u16,)|Ok(Enum::Variant2(< <variants::Variant2<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant2<T> ,vCurrent> > ::backwards:: < < <variants::Variant2<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant2<T> ,vCurrent> > ::BackwardsError, >(variants::Variant2:: <T> ::intro({
                let _ = core::marker::PhantomData:: <(u8,u16,)> ;
                x
            },Default::default(),)).map_err(GenericBackwardsError:: <T> ::Variant2).map_err(E::from)?)), |x: (T::XY,)|Ok(Enum::Variant3(< <variants::Variant3<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant3<T> ,vCurrent> > ::backwards:: < < <variants::Variant3<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant3<T> ,vCurrent> > ::BackwardsError, >(variants::Variant3:: <T> ::intro({
                let _ = core::marker::PhantomData:: <(T::XY,)> ;
                x
            },Default::default(),)).map_err(GenericBackwardsError:: <T> ::Variant3).map_err(E::from)?)), |myfield: u8,field2: (T::XY,T::XY), |Ok(Enum::Variant4(< <variants::Variant4<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant4<T> ,vCurrent> > ::backwards:: < < <variants::Variant4<T>as HasGenericVariant> ::MigrationFromGeneric as Migration<variants::Variant4<T> ,vCurrent> > ::BackwardsError, >(variants::Variant4:: <T> ::intro(myfield,field2,Default::default(),)).map_err(GenericBackwardsError:: <T> ::Variant4).map_err(E::from)?)),)
		}
	}
}

// ///////////////////////////////////////////
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
