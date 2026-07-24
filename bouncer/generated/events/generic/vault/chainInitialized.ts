import { arbitrumVaultChainInitializedEvent } from '../../arbitrumVault/chainInitialized';
import { assethubVaultChainInitializedEvent } from '../../assethubVault/chainInitialized';
import { bitcoinVaultChainInitializedEvent } from '../../bitcoinVault/chainInitialized';
import { bscVaultChainInitializedEvent } from '../../bscVault/chainInitialized';
import { ethereumVaultChainInitializedEvent } from '../../ethereumVault/chainInitialized';
import { polkadotVaultChainInitializedEvent } from '../../polkadotVault/chainInitialized';
import { solanaVaultChainInitializedEvent } from '../../solanaVault/chainInitialized';
import { tronVaultChainInitializedEvent } from '../../tronVault/chainInitialized';

export const vaultChainInitializedEvent = {
  Arbitrum: arbitrumVaultChainInitializedEvent,
  Assethub: assethubVaultChainInitializedEvent,
  Bitcoin: bitcoinVaultChainInitializedEvent,
  Bsc: bscVaultChainInitializedEvent,
  Ethereum: ethereumVaultChainInitializedEvent,
  Polkadot: polkadotVaultChainInitializedEvent,
  Solana: solanaVaultChainInitializedEvent,
  Tron: tronVaultChainInitializedEvent,
} as const;
