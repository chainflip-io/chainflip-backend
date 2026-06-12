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
use sp_core::{bounded::BoundedVec, Get};

pub fn map_bounded_vec<S: Get<u32>, T0, T1>(
	xs: BoundedVec<T0, S>,
	f: impl Fn(T0) -> T1,
) -> BoundedVec<T1, S> {
	BoundedVec::truncate_from(xs.into_iter().map(f).collect())
}
