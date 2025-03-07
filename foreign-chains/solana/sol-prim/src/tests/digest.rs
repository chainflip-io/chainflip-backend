// Copyright 2025 Chainflip Labs GmbH and Anza Maintainers <maintainers@anza.xyz>
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

#[cfg(feature = "str")]
#[cfg(test)]
mod from_and_to_str {
	use core::fmt::Write;

	use crate::{digest::Digest, utils::WriteBuffer};

	#[test]
	fn round_trip() {
		let mut write_buf = WriteBuffer::new([0u8; 1024]);
		for input in [
			"EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG",
			"4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY",
			"5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d",
		] {
			write_buf.reset();

			let parsed: Digest = input.parse().expect("parse error");
			write!(write_buf, "{}", parsed).expect("write-buffer error");

			assert_eq!(write_buf.as_ref(), input.as_bytes());
		}
	}
}
