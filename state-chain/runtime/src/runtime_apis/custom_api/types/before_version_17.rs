use super::*;

/// Combined ingress and egress events, returned by `cf_ingress_egress_events` in API versions
/// up to and including v16. In v17 the return type changed to `(RawIngressEvents,
/// RawEgressEvents)`.
#[derive(Clone, Debug, TypeInfo, Encode, Decode)]
pub enum RawWitnessedEvents {
	Bitcoin {
		deposits: Vec<(u64, DepositWitness<Bitcoin>)>,
		vault_deposits: Vec<(u64, VaultDepositWitness<Runtime, BitcoinInstance>)>,
		broadcasts: Vec<(u64, TransactionConfirmation<Runtime, BitcoinInstance>)>,
	},
	Ethereum {
		deposits: Vec<(u64, DepositWitness<Ethereum>)>,
		vault_deposits: Vec<(u64, EvmVaultContractEvent<Runtime, EthereumInstance>)>,
		broadcasts: Vec<(u64, EvmKeyManagerEvent<Runtime, EthereumInstance>)>,
	},
	Arbitrum {
		deposits: Vec<(u64, DepositWitness<Arbitrum>)>,
		vault_deposits: Vec<(u64, EvmVaultContractEvent<Runtime, ArbitrumInstance>)>,
		broadcasts: Vec<(u64, EvmKeyManagerEvent<Runtime, ArbitrumInstance>)>,
	},
}
