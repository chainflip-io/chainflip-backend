#[cfg(feature = "str")]
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
