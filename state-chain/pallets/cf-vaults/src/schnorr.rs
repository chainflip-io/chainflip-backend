use codec::{Decode, Encode, EncodeLike, Input, Output};
use frame_support::RuntimeDebug;
use libsecp256k1::Signature;

/// Schnorr Signature type
#[derive(PartialEq, Eq, Clone, RuntimeDebug)]
pub struct SchnorrSignature(Signature);

impl Encode for SchnorrSignature {
	fn encode_to<T: Output + ?Sized>(&self, output: &mut T) {
		output.write(&self.0.serialize());
	}
}

impl EncodeLike for SchnorrSignature {}

impl Decode for SchnorrSignature {
	fn decode<I: Input>(input: &mut I) -> Result<Self, codec::Error> {
		let mut serialized: [u8; 64] = [0; 64];
		input.read(&mut serialized)?;
		Signature::parse_standard(&serialized)
			.map(|sig| SchnorrSignature(sig))
			.map_err::<codec::Error, _>(|_| "decoding failed at public key".into())
	}
}
