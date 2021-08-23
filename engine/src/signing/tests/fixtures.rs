use std::convert::TryInto;

use lazy_static::lazy_static;

lazy_static! {
  pub static ref MESSAGE: [u8; 32] = "Chainflip:Chainflip:Chainflip:01"
      .as_bytes()
      .try_into()
      .unwrap();
      /// Just in case we need to test signing two messages
  pub static ref MESSAGE2: [u8; 32] = "Chainflip:Chainflip:Chainflip:02"
      .as_bytes()
      .try_into()
      .unwrap();
}
