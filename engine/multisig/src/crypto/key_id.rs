use cf_primitives::EpochIndex;

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
pub struct KeyId {
	pub epoch_index: EpochIndex,
	pub public_key_bytes: Vec<u8>,
}

/// Defines the commonly agreed-upon byte-encoding used for public keys.
pub trait CanonicalEncoding {
	fn encode_key(&self) -> Vec<u8>;
}

impl<Key> From<(EpochIndex, Key)> for KeyId
where
	Key: CanonicalEncoding,
{
	fn from((epoch_index, key): (EpochIndex, Key)) -> Self {
		KeyId { epoch_index, public_key_bytes: key.encode_key() }
	}
}

impl CanonicalEncoding for cf_chains::dot::PolkadotPublicKey {
	fn encode_key(&self) -> Vec<u8> {
		self.0.to_vec()
	}
}

impl CanonicalEncoding for cf_chains::btc::AggKey {
	fn encode_key(&self) -> Vec<u8> {
		self.pubkey_x.to_vec()
	}
}

// TODO: remove this.
impl KeyId {
	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend_from_slice(&self.epoch_index.to_be_bytes());
		bytes.extend_from_slice(&self.public_key_bytes);
		bytes
	}

	pub fn from_bytes(bytes: &[u8]) -> Self {
		const S: usize = core::mem::size_of::<EpochIndex>();
		let epoch_index = EpochIndex::from_be_bytes(bytes[..S].try_into().unwrap());
		let public_key_bytes = bytes[S..].to_vec();
		Self { epoch_index, public_key_bytes }
	}
}

impl core::fmt::Display for KeyId {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		#[cfg(feature = "std")]
		{
			write!(
				f,
				"KeyId(epoch_index: {}, public_key_bytes: {})",
				self.epoch_index,
				hex::encode(self.public_key_bytes.clone())
			)
		}
		#[cfg(not(feature = "std"))]
		{
			write!(
				f,
				"KeyId(epoch_index: {}, public_key_bytes: {:?})",
				self.epoch_index, self.public_key_bytes
			)
		}
	}
}

#[test]
fn test_key_id_to_and_from_bytes() {
	let key_ids = [
		KeyId { epoch_index: 0, public_key_bytes: vec![] },
		KeyId { epoch_index: 1, public_key_bytes: vec![1, 2, 3] },
		KeyId { epoch_index: 22, public_key_bytes: vec![0xa, 93, 145, u8::MAX, 0] },
	];

	for key_id in key_ids {
		assert_eq!(key_id, KeyId::from_bytes(&key_id.to_bytes()));
	}

	let key_id = KeyId {
		epoch_index: 29,
		public_key_bytes: vec![
			0xa,
			93,
			141,
			u8::MAX,
			0,
			82,
			2,
			39,
			144,
			241,
			29,
			91,
			3,
			241,
			120,
			194,
		],
	};
	// We check this because if this form changes then there will be an impact to how keys should be
	// loaded from the db on the CFE. Thus, we want to be notified if this changes.
	let expected_bytes =
		vec![0, 0, 0, 29, 10, 93, 141, 255, 0, 82, 2, 39, 144, 241, 29, 91, 3, 241, 120, 194];
	assert_eq!(expected_bytes, key_id.to_bytes());
	assert_eq!(key_id, KeyId::from_bytes(&expected_bytes));
}
