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

#![cfg(test)]

use crate::migrations::HasChangelog;

#[cf_proc_macros::generate_module]
pub struct MyS<T> {
	pub a: T,
}

impl<T: HasChangelog> HasChangelog for MyS<T> {
	type if_unspecified = _MyS::see_field_changelogs;
}

pub trait T1 {
	type XY;
}

mod enum1 {
	use super::T1;

	cf_utilities::generate_module! {
		pub enum MyTestValuesWithoutChangelog<T: T1> {
			Variant1(_0: T::XY),
			VariantNone{_0: u8,_1: u16, _2: u8,},
			Variant3(_0: T::XY),
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
		Variant5 {
			myfield: u8,
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
		Variant3(_0: T::XY),
		Variant5 {
			myfield: u8,
			field2: (T::XY, T::XY),
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
		Variant2{_0: u8, _1: u16, _2: u8, _3: u8, _4: u8, _5: u8, _6: u16, _7: u8, },
		Variant3 {_0: T::XY, _1: T::XY,},
		Variant4 {
			field2: T::XY,
			field3: T::XY,
			field4: T::XY,
			field5: T::XY,
		},
		Variant5 (
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
		type in_20100 =
			_MyTestValues::see_variant_changelogs_and_also<_MyTestValues::variant::Variant2::Added>;
	}
}
