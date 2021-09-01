use codec::{Decode, Encode, EncodeLike, Input, Output};
use frame_support::RuntimeDebug;

impl Encode for SchnorrSignature {
	fn encode_to<T: Output + ?Sized>(&self, output: &mut T) {
		output.write(&self.s);
		output.write(&self.r.serialize());
	}
}

impl EncodeLike for SchnorrSignature {}

impl Decode for SchnorrSignature {
	fn decode<I: Input>(input: &mut I) -> Result<Self, codec::Error> {
		// Our fixed size buffer for the scalar property
		let mut scalar: [u8; 32] = [0; 32];
		input.read(&mut scalar)?;
		// We expect a compressed buffer here of 33 bytes long
		let mut pk: [u8; 33] = [0; 33];
		input.read(&mut pk)?;
		let pk = secp256k1::PublicKey::from_slice(&pk)
			.map_err::<codec::Error, _>(|_| "decoding failed at public key".into())?;

		Ok(SchnorrSignature { s: scalar, r: pk })
	}
}

/// Schnorr Signature type
#[derive(PartialEq, Eq, Clone, RuntimeDebug)]
pub struct SchnorrSignature {
	/// Scalar component
	// s: secp256k1::SecretKey,
	pub s: [u8; 32],
	/// Point component
	pub r: secp256k1::PublicKey,
}

#[cfg(test)]
pub fn create_valid_schnorr_signature() -> SchnorrSignature {
	let public_key = secp256k1::PublicKey::from_slice(&[
		3, 23, 183, 225, 206, 31, 159, 148, 195, 42, 67, 115, 146, 41, 248, 140, 11, 3, 51, 41,
		111, 180, 110, 143, 114, 134, 88, 73, 198, 174, 52, 184, 78,
	])
	.expect("Valid public key");

	SchnorrSignature {
		s: [1; 32],
		r: public_key,
	}
}

#[test]
fn encode_and_decode_a_schnorr_signature() {
	let signature = create_valid_schnorr_signature();
	let signature_after_decoding =
		SchnorrSignature::decode(&mut signature.encode().as_slice()).expect("Decode signature");

	assert_eq!(signature, signature_after_decoding);
}
