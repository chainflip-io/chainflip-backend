import { arbitrumVaultActivationTxFailedAwaitingGovernanceEvent } from '../../arbitrumVault/activationTxFailedAwaitingGovernance';
import { assethubVaultActivationTxFailedAwaitingGovernanceEvent } from '../../assethubVault/activationTxFailedAwaitingGovernance';
import { bitcoinVaultActivationTxFailedAwaitingGovernanceEvent } from '../../bitcoinVault/activationTxFailedAwaitingGovernance';
import { bscVaultActivationTxFailedAwaitingGovernanceEvent } from '../../bscVault/activationTxFailedAwaitingGovernance';
import { ethereumVaultActivationTxFailedAwaitingGovernanceEvent } from '../../ethereumVault/activationTxFailedAwaitingGovernance';
import { polkadotVaultActivationTxFailedAwaitingGovernanceEvent } from '../../polkadotVault/activationTxFailedAwaitingGovernance';
import { solanaVaultActivationTxFailedAwaitingGovernanceEvent } from '../../solanaVault/activationTxFailedAwaitingGovernance';
import { tronVaultActivationTxFailedAwaitingGovernanceEvent } from '../../tronVault/activationTxFailedAwaitingGovernance';

export const vaultActivationTxFailedAwaitingGovernanceEvent = {
  Arbitrum: arbitrumVaultActivationTxFailedAwaitingGovernanceEvent,
  Assethub: assethubVaultActivationTxFailedAwaitingGovernanceEvent,
  Bitcoin: bitcoinVaultActivationTxFailedAwaitingGovernanceEvent,
  Bsc: bscVaultActivationTxFailedAwaitingGovernanceEvent,
  Ethereum: ethereumVaultActivationTxFailedAwaitingGovernanceEvent,
  Polkadot: polkadotVaultActivationTxFailedAwaitingGovernanceEvent,
  Solana: solanaVaultActivationTxFailedAwaitingGovernanceEvent,
  Tron: tronVaultActivationTxFailedAwaitingGovernanceEvent,
} as const;
