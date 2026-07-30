import { arbitrumVaultVaultRotatedExternallyEvent } from '../../arbitrumVault/vaultRotatedExternally';
import { assethubVaultVaultRotatedExternallyEvent } from '../../assethubVault/vaultRotatedExternally';
import { bitcoinVaultVaultRotatedExternallyEvent } from '../../bitcoinVault/vaultRotatedExternally';
import { bscVaultVaultRotatedExternallyEvent } from '../../bscVault/vaultRotatedExternally';
import { ethereumVaultVaultRotatedExternallyEvent } from '../../ethereumVault/vaultRotatedExternally';
import { polkadotVaultVaultRotatedExternallyEvent } from '../../polkadotVault/vaultRotatedExternally';
import { solanaVaultVaultRotatedExternallyEvent } from '../../solanaVault/vaultRotatedExternally';
import { tronVaultVaultRotatedExternallyEvent } from '../../tronVault/vaultRotatedExternally';

export const vaultVaultRotatedExternallyEvent = {
  Arbitrum: arbitrumVaultVaultRotatedExternallyEvent,
  Assethub: assethubVaultVaultRotatedExternallyEvent,
  Bitcoin: bitcoinVaultVaultRotatedExternallyEvent,
  Bsc: bscVaultVaultRotatedExternallyEvent,
  Ethereum: ethereumVaultVaultRotatedExternallyEvent,
  Polkadot: polkadotVaultVaultRotatedExternallyEvent,
  Solana: solanaVaultVaultRotatedExternallyEvent,
  Tron: tronVaultVaultRotatedExternallyEvent,
} as const;
