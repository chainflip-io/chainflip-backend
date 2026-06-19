import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronVaultVaultRotatedExternally = cfChainsEvmAggKey;

export const tronVaultVaultRotatedExternallyEvent = defineEvent(
  'TronVault.VaultRotatedExternally',
  tronVaultVaultRotatedExternally,
);
