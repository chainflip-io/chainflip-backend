import { arbitrumVaultAwaitingGovernanceActivationEvent } from '../../arbitrumVault/awaitingGovernanceActivation';
import { assethubVaultAwaitingGovernanceActivationEvent } from '../../assethubVault/awaitingGovernanceActivation';
import { bitcoinVaultAwaitingGovernanceActivationEvent } from '../../bitcoinVault/awaitingGovernanceActivation';
import { bscVaultAwaitingGovernanceActivationEvent } from '../../bscVault/awaitingGovernanceActivation';
import { ethereumVaultAwaitingGovernanceActivationEvent } from '../../ethereumVault/awaitingGovernanceActivation';
import { polkadotVaultAwaitingGovernanceActivationEvent } from '../../polkadotVault/awaitingGovernanceActivation';
import { solanaVaultAwaitingGovernanceActivationEvent } from '../../solanaVault/awaitingGovernanceActivation';
import { tronVaultAwaitingGovernanceActivationEvent } from '../../tronVault/awaitingGovernanceActivation';

export const vaultAwaitingGovernanceActivationEvent = {
  Arbitrum: arbitrumVaultAwaitingGovernanceActivationEvent,
  Assethub: assethubVaultAwaitingGovernanceActivationEvent,
  Bitcoin: bitcoinVaultAwaitingGovernanceActivationEvent,
  Bsc: bscVaultAwaitingGovernanceActivationEvent,
  Ethereum: ethereumVaultAwaitingGovernanceActivationEvent,
  Polkadot: polkadotVaultAwaitingGovernanceActivationEvent,
  Solana: solanaVaultAwaitingGovernanceActivationEvent,
  Tron: tronVaultAwaitingGovernanceActivationEvent,
} as const;
