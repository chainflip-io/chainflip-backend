#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub enum BitcoinApi<Environment: 'static> {
	BatchFetchAndTransfer(batch_fetch_and_transfer::BatchFetchAndTransfer),
	//RotateVaultProxy(rotate_vault_proxy::RotateVaultProxy),
	//CreateAnonymousVault(create_anonymous_vault::CreateAnonymousVault),
	//ChangeGovKey(set_gov_key_with_agg_key::ChangeGovKey),
	#[doc(hidden)]
	#[codec(skip)]
	_Phantom(PhantomData<Environment>, Never),
}

impl<E> AllBatch<Bitcoin> for BitcoinApi<E>
where
	E: ChainEnvironment<<Bitcoin as Chain>::Amount, Vec<Utxo>>
		+ ChainEnvironment<(), BitcoinNetwork>,
{
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Bitcoin>>,
		transfer_params: Vec<TransferAssetParams<Bitcoin>>,
	) -> Result<Self, ()> {
		let bitcoin_network = <E as ChainEnvironment<(), BitcoinNetwork>>::lookup()
			.expect("Since the lookup function always returns a some");
		let total_output_amount = 0;
		let btc_outputs = vec![];
		for transfer_param in transfer_params {
			btc_outputs.push(BitcoinOutput {
				amount: transfer_param.clone().amount,
				script_pubkey: scriptpubkey_from_address(
					&std::str::from_utf8(&transfer_param.to[..]).map_err(|_| ())?,
					bitcoin_network,
				)?,
			});
			total_output_amount += transfer_param.amount;
		}
		let selected_input_utxos =
			<E as ChainEnvironment<Amount, Vec<Utxo>>>::lookup(total_output_amount).ok_or(())?;

		Ok(Self::BatchFetchAndTransfer(
			batch_fetch_and_transfer::BatchFetchAndTransfer::new_unsigned(
				selected_input_utxos,
				btc_outputs,
			),
		))
	}
}

impl<E> From<batch_fetch_and_transfer::BatchFetchAndTransfer> for BitcoinApi<E> {
	fn from(tx: batch_fetch_and_transfer::BatchFetchAndTransfer) -> Self {
		Self::BatchFetchAndTransfer(tx)
	}
}

impl<E> ApiCall<Polkadot> for BitcoinApi<E> {
	fn threshold_signature_payload(&self) -> <Polkadot as ChainCrypto>::Payload {
		match self {
			BitcoinApi::BatchFetchAndTransfer(tx) => tx.threshold_signature_payload(),

			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}

	fn signed(self, threshold_signature: &<Polkadot as ChainCrypto>::ThresholdSignature) -> Self {
		match self {
			BitcoinApi::BatchFetchAndTransfer(call) => call.signed(threshold_signature).into(),

			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}

	fn chain_encoded(&self) -> Vec<u8> {
		match self {
			BitcoinApi::BatchFetchAndTransfer(call) => call.chain_encoded(),

			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}

	fn is_signed(&self) -> bool {
		match self {
			BitcoinApi::BatchFetchAndTransfer(call) => call.is_signed(),

			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}
}
