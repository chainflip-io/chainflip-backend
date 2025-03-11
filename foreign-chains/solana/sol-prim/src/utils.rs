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

#![allow(unused)]

#[derive(Debug)]
pub struct WriteBuffer<B> {
	buffer: B,
	offset: usize,
}

impl<B> WriteBuffer<B> {
	pub fn new(buffer: B) -> Self {
		Self { buffer, offset: 0 }
	}

	pub fn reset(&mut self) {
		self.offset = 0;
	}
}

impl<B> AsRef<[u8]> for WriteBuffer<B>
where
	B: AsRef<[u8]>,
{
	fn as_ref(&self) -> &[u8] {
		&self.buffer.as_ref()[..self.offset]
	}
}

impl<B> core::fmt::Write for WriteBuffer<B>
where
	B: AsMut<[u8]>,
{
	fn write_str(&mut self, s: &str) -> core::fmt::Result {
		let src = s.as_bytes();
		let dst = &mut self.buffer.as_mut()[self.offset..][..src.len()];

		dst.copy_from_slice(src);
		self.offset += src.len();

		Ok(())
	}
}
