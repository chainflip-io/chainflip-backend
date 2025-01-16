pub mod broker_crypto {
	use sp_application_crypto::{app_crypto, sr25519, KeyTypeId};
	/// Broker Key Type ID used to store the key on state chain node keystore
	pub const BROKER_KEY_TYPE_ID: KeyTypeId = KeyTypeId(*b"brok");

	app_crypto!(sr25519, BROKER_KEY_TYPE_ID);
}
pub mod lp_crypto {
	use sp_application_crypto::{app_crypto, sr25519, KeyTypeId};
	/// Liquidity Provider Key Type ID used to store the key on state chain node keystore
	pub const LP_KEY_TYPE_ID: KeyTypeId = KeyTypeId(*b"lqpr");

	app_crypto!(sr25519, LP_KEY_TYPE_ID);
}
