use crate::*;
pub mod eth;


use sp_runtime::{generic::SignedPayload, MultiAddress, MultiSignature};
pub use polkadot_runtime::Runtime;


pub type PolkadotRuntime = polkadot_runtime::Runtime; //we need to import the latest polkadot runtime here

pub type PolkadotRuntimeCall = polkadot_runtime::RuntimeCall; //This needs to be imported from the current runtime of polkadot

//pub type PolkadotAggkey = PolkadotAccount; //Currently, we are using the same Aggkey (or same framework with a different key) to sign for
									   // polkadot transaction as we do for ethereum transactions

pub type PolkadotGovKey = eth::Address; //Same as above

pub type PolkadotBlockHashCount = todo!(); //import from runtime common types crate in polkadot repo

pub type PolkadotAddress = MultiAddress<PolkadotRuntime::AccountId, ()>; //import this type
// from multiaddress.rs
pub type PolkadotSignature = MultiSignature;

pub type PolkadotUncheckedExtrinsic = generic::UncheckedExtrinsic<PolkadotAddress, PolkadotRuntimeCall, PolkadotSignature, PolkadotSignedExtra>;

pub type PolkadotSignedExtra = (  //import from polkadot runtime
	frame_system::CheckNonZeroSender<Runtime>,
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckMortality<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
	claims::PrevalidateAttests<Runtime>,
); //copied from the latest version of polkadot runtime, but this needs to be also imported directly
   // from polkadot repo dynamically

/// The payload being signed in transactions.
pub type PolkadotPayload = generic::SignedPayload<PolkadotRuntimeCall, PolkadotSignedExtra>;
pub type EncodedPolkadotPayload = &[u8];

pub type PolkadotLookup = <PolkadotRuntime as frame_system::Config>::Lookup;


pub struct Polkadot;

impl Chain for Polkadot {
	type ChainBlockNumber = u64;
	type ChainAmount = u128;
	type TrackedData = todo!();
	type ChainAccount = PolkadotRuntime::AccountId;
	type ChainAsset = ();
}

impl ChainCrypto for Polkadot {
	type AggKey = <Polkadot as Chain>::ChainAccount;
	type Payload = EncodedPolkadotPayload;
	type ThresholdSignature = PolkadotSignature;
	type TransactionHash = ();
	type GovKey = PolkadotGovKey;

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool {
		signature.verify(payload, agg_key)
	}

	fn agg_key_to_payload(agg_key: Self::AggKey) -> Self::Payload {
		//H256(Blake2_256::hash(&agg_key.to_pubkey_compressed()))
        todo!();
	}
}

impl ChainAbi for Polkadot {
	type UnsignedTransaction = extrinsic::UncheckedExtrinsic<PolkadotRuntime>;
	type SignedTransaction = extrinsic::CheckedExtrinsic<PolkadotRuntime>;
	type SignerCredential = PolkadotRuntime::AccountId; // Depending on how we structure the process of transaction submission polkadot (two step
													// process or one), we might or might not need this type -> discussion
	type ReplayProtection = todo!();
	type ValidationError = todo!();

	fn verify_signed_transaction(
		unsigned_tx: &Self::UnsignedTransaction,
		signed_tx: &Self::SignedTransaction,
		signer_credential: &Self::SignerCredential,
	) -> Result<Self::TransactionHash, Self::ValidationError> {
		todo!(); //<UncheckedExtrinsic as Checkable>::check()
	}
}

#[derive(Encode, Decode, TypeInfo, Copy, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct PolkadotExtrinsicSignatureHandler {
    vault_account: Polkadot::ChainAccount,
	extrinsic_call: PolkadotRuntimeCall,
	signed_extrinsic: Polkadot::SignedTransaction,
	signature_payload: Polkadot::Payload,
	nonce: <PolkadotRuntime as frame_system::Config>::Index,
    extra: PolkadotSignedExtra,
}

impl ExtrinsicSignatureHandler {
	pub fn new_empty(nonce: <PolkadotRuntime as frame_system::Config>::Index, vault_account: Polkadot::ChainAccount) -> Self {
		Self { 
            nonce:nonce,
            vault_account:vault_account,
            ..Default::default() 
            }
	}
	pub fn insert_extrinsic_call(&mut self, extrinsic_call: PolkadotRuntimeCall) {
		self.extrinsic_call = extrinsic_call;
	}

	pub fn insert_and_get_threshold_signature_payload(&self) -> Polkadot::Payload {
        use sp_runtime::traits::StaticLookup;
		// take the biggest period possible.
		let period =
			PolkadotBlockHashCount::get().checked_next_power_of_two().map(|c| c / 2).unwrap_or(2) as u64;

		let current_block = frame_system::Pallet<PolkadotRuntime>::block_number()
			.saturated_into::<u64>()
			// The `System::block_number` is initialized with `n+1`,
			// so the actual block number is `n`.
			.saturating_sub(1);
		let tip = 0;
		let extra: PolkadotSignedExtra = (
			frame_system::CheckNonZeroSender::<PolkadotRuntime>::new(),
			frame_system::CheckSpecVersion::<PolkadotRuntime>::new(),
			frame_system::CheckTxVersion::<PolkadotRuntime>::new(),
			frame_system::CheckGenesis::<PolkadotRuntime>::new(),
			frame_system::CheckMortality::<PolkadotRuntime>::from(generic::Era::mortal(
				period,
				current_block,
			)),
			frame_system::CheckNonce::<PolkadotRuntime>::from(self.nonce),
			frame_system::CheckWeight::<PolkadotRuntime>::new(),
			pallet_transaction_payment::ChargeTransactionPayment::<PolkadotRuntime>::from(tip),
			claims::PrevalidateAttests::<PolkadotRuntime>::new(),
		);
		let raw_payload = PolkadotPayload::new(self.extrinsic_call, extra)
			.map_err(|e| {
				log::warn!("Unable to create signed payload: {:?}", e);
			})
			.ok()?;
        self.signature_payload = raw_payload.using_encoded(|encoded_payload| encoded_payload);
        self.extra = extra;

        self.signature_payload
    }

	pub fn insert_and_get_signed_unchecked_extrinsic(&mut self, signature: <Polkadot as ChainCrypto>::ThresholdSignature) -> PolkadotUncheckedExtrinsic {
        self.signed_extrinsic = PolkadotRuntime::PolkadotUncheckedExtrinsic::new_signed(self.extrinsic_call, PolkadotAddress::Id(self.vault_account.clone()), signature, self.extra )
    }
	pub fn is_signed(&self) -> bool {
        match self.signature {
			Some((signed, signature, extra)) => {
				let raw_payload = SignedPayload::new(self.extrinsic_call, self.extra)?;
				if !raw_payload.using_encoded(|payload| signature.verify(payload, &self.vault_account)) {
					false
				}
                else {
                    true
                }
            },
			None => false,
		}
    }
}
