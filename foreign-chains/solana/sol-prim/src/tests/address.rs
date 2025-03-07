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

#![cfg(test)]

#[cfg(feature = "str")]
#[cfg(test)]
mod from_and_to_str {
	use core::fmt::Write;

	use crate::{address::Address, consts, utils::WriteBuffer};

	#[test]
	fn zero_address_from_str() {
		assert_eq!(
			"11111111111111111111111111111111".parse::<Address>().expect("parse error"),
			Address([0; consts::SOLANA_ADDRESS_LEN])
		);
	}

	#[test]
	fn zero_address_to_str() {
		let mut buf = WriteBuffer::new([0u8; 1024]);
		write!(buf, "{}", Address([0; consts::SOLANA_ADDRESS_LEN])).expect("write");
		assert_eq!(buf.as_ref(), "11111111111111111111111111111111".as_bytes(),);
	}

	#[test]
	fn round_trip() {
		let mut write_buf = WriteBuffer::new([0u8; 1024]);
		for input in [
			"96yeNG1KYJKAVnfKqfkfktkXuPj1CLPEsgCDkm42VcaT",
			"7TecQdLbPuxt3mWukbZ1g1dTZeA6rxgjMxfS9MRURaEP",
			"dCmA3wzpw4CvHLR1ynjStbYx8ZwxtLVkFQmsG3F3b37",
			"ARdmZ4WrV8pnsjtCa4V67zv8vTUTmF798UPvmnkTZ3Gx",
		] {
			write_buf.reset();

			let parsed: Address = input.parse().expect("parse error");
			write!(write_buf, "{}", parsed).expect("write-buffer error");

			assert_eq!(write_buf.as_ref(), input.as_bytes());
		}
	}
}

#[cfg(feature = "serde")]
#[cfg(test)]
mod feature_serde {
	use crate::{address::Address, consts};

	#[test]
	fn zero_address_to_json() {
		let addr = Address([0u8; consts::SOLANA_ADDRESS_LEN]);
		assert_eq!(
			serde_json::to_string(&addr).expect("serialize"),
			"\"11111111111111111111111111111111\""
		);
	}

	#[test]
	fn zero_address_from_json() {
		let addr: Address =
			serde_json::from_str("\"11111111111111111111111111111111\"").expect("deserialize");
		assert_eq!(addr, Address([0u8; consts::SOLANA_ADDRESS_LEN]));
	}
}
