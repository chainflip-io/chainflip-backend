import { arbitrumVaultVaultActivationCompletedEvent } from '../../arbitrumVault/vaultActivationCompleted';
import { assethubVaultVaultActivationCompletedEvent } from '../../assethubVault/vaultActivationCompleted';
import { bitcoinVaultVaultActivationCompletedEvent } from '../../bitcoinVault/vaultActivationCompleted';
import { bscVaultVaultActivationCompletedEvent } from '../../bscVault/vaultActivationCompleted';
import { ethereumVaultVaultActivationCompletedEvent } from '../../ethereumVault/vaultActivationCompleted';
import { polkadotVaultVaultActivationCompletedEvent } from '../../polkadotVault/vaultActivationCompleted';
import { solanaVaultVaultActivationCompletedEvent } from '../../solanaVault/vaultActivationCompleted';
import { tronVaultVaultActivationCompletedEvent } from '../../tronVault/vaultActivationCompleted';

export const vaultVaultActivationCompletedEvent = {
  Arbitrum: arbitrumVaultVaultActivationCompletedEvent,
  Assethub: assethubVaultVaultActivationCompletedEvent,
  Bitcoin: bitcoinVaultVaultActivationCompletedEvent,
  Bsc: bscVaultVaultActivationCompletedEvent,
  Ethereum: ethereumVaultVaultActivationCompletedEvent,
  Polkadot: polkadotVaultVaultActivationCompletedEvent,
  Solana: solanaVaultVaultActivationCompletedEvent,
  Tron: tronVaultVaultActivationCompletedEvent,
} as const;
