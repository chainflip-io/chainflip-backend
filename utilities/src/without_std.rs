pub mod conditional;

pub const fn bs58_array<const S: usize>(s: &'static str) -> [u8; S] {
	bs58::decode(s.as_bytes()).into_array_const_unwrap()
}
