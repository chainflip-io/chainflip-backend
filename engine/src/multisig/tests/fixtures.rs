use lazy_static::lazy_static;

use crate::multisig::MessageHash;

lazy_static! {
	pub static ref MESSAGE: Vec<u8> = "Chainflip:Chainflip:Chainflip:01".as_bytes().to_vec();
	pub static ref MESSAGE_HASH: MessageHash = MessageHash(MESSAGE.clone());
}
