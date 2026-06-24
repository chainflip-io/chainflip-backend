import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronVaultChainInitialized = z.null();

export const tronVaultChainInitializedEvent = defineEvent(
  'TronVault.ChainInitialized',
  tronVaultChainInitialized,
);
