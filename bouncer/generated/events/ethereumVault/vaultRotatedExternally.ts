import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumVaultVaultRotatedExternally = cfChainsEvmAggKey;

export const ethereumVaultVaultRotatedExternallyEvent = defineEvent(
  'EthereumVault.VaultRotatedExternally',
  ethereumVaultVaultRotatedExternally,
);
