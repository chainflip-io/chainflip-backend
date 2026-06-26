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
pub enum MyTestValues<T: T1> {
	Variant1(T::XY),
	// Variant2(u8,u16),
	// Variant3(T::XY),
	// Variant4 {
	// 	myfield: u8,
	// 	field2: (T::XY, T::XY, u16),
	// },
}
// 	mod _MyTestValues { #![migrations] }
// }

// type XX = _MyTestValues::variants::Variant1<u8>;

// duplicate::substitute! {
// 	[
// 		typ1 [T: T1, T2: Copy];
// 	]
// 	type X<typ1> = T::XY;
// }

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
		mod (B: Clone) {
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
