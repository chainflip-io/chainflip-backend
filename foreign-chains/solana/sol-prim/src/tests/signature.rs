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
	use crate::{signature::Signature, utils::WriteBuffer};
	use core::fmt::Write;

	#[test]
	fn round_trip() {
		let mut write_buf = WriteBuffer::new([0u8; 1024]);
		for input in [
			"5cKt1H4Yn7LLJ2Jh8gudHYq3xaaSFoZh4U8TVouHe1o9KJ2dqfd6kKNAfKgnxpjr4fWBb8AnrSnrs4Z9fq9qeCth",
			"46vy3sp4k5pQDjVymzrD58L4strx5vmK5B9pjsEuNcXKfaZpWie5r6bQYnrzpu3giaZL1b8NmFhDDhz9U3bTgQkP",
			"3BRnMuqZBXfYoniRr1aNYYJSJ8axqXCEhCLdEgrTuiRV45ps9Jkd82QGbZDcx99aYTnvxd6tvw9Z6AcPvuzyeAVC",
			"31k6ZmFd8hV8fYwbh3M4jGHfv7HgmJk1bmBZoYHeFZbXVLbUkbvdi5Vvf8Q2dKQrXkQsUzgocxtYmRHQJb5dbd9L",
		] {
			write_buf.reset();

			let parsed: Signature = input.parse().expect("parse error");
			write!(write_buf, "{}", parsed).expect("write-buffer error");

			assert_eq!(write_buf.as_ref(), input.as_bytes());
		}
	}
}
