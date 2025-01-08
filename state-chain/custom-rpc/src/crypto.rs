pub mod broker_crypto {
	use sp_application_crypto::{app_crypto, sr25519, KeyTypeId};
	/// Broker Key Type ID used to store the key on state chain node keystore
	pub const BROKER_ID_KEY: KeyTypeId = KeyTypeId(*b"brok");

	app_crypto!(sr25519, BROKER_ID_KEY);
}
