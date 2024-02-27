#[cfg(feature = "str")]
mod from_and_to_str {
	use core::fmt::Write;

	use crate::{signature::Signature, utils::WriteBuffer};

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
