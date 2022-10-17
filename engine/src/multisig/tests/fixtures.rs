use lazy_static::lazy_static;

use crate::multisig::MessageHash;

lazy_static! {
	pub static ref MESSAGE: [u8; 32] =
		"Chainflip:Chainflip:Chainflip:01".as_bytes().try_into().unwrap();
	pub static ref MESSAGE_HASH: MessageHash = MessageHash(*MESSAGE);
}
