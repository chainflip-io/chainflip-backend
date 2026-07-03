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
#![allow(unused)]

pub trait T1 {
	type XY;
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
