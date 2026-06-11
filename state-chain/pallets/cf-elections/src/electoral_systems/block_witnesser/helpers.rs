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
#[macro_export]
macro_rules! prop_do {
    (let $var:pat in $expr:expr; $($expr2:tt)+) => {
        $expr.prop_flat_map(move |$var| prop_do!($($expr2)+))
    };
    (return $($rest:tt)+) => {
        Just($($rest)+)
    };
	($ctor:ident {
		$($field:ident : $strat:expr,)*
	}) => {
		( $($strat,)* ).prop_map(
			|($($field,)*)| $ctor {
				$($field,)*
			}
		)
	};
    ($expr:expr) => {$expr};
    (let $var:pat = $expr:expr; $($expr2:tt)+ ) => {
        {
            let $var = $expr;
            prop_do!($($expr2)+)
        }
    };
    ($var:ident <<= $expr:expr; $($expr2:tt)+) => {
        $expr.prop_flat_map(move |$var| prop_do!($($expr2)+))
    };
}

#[macro_export]
macro_rules! asserts {
	($description:tt in $expr:expr; $($tail:tt)*) => {
		assert!($expr, $description);
		asserts!{$($tail)*}
	};
	($description:tt in $expr:expr, else $($vars:expr), +; $($tail:tt)*) => {
		assert!($expr, $description, $($vars), +);
		asserts!{$($tail)*}
	};
	($description:tt in $expr:expr, where {$($tt:tt)*} $($tail:tt)*) => {
		{
			$($tt)*
			assert!($expr, $description);
		}
		asserts!{$($tail)*}
	};
	(let $ident:ident = $expr:expr; $($tail:tt)*) => {
		let $ident = $expr;
		asserts!{$($tail)*}
	};
	() => {}
}
