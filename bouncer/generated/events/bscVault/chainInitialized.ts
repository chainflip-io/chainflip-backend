import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscVaultChainInitialized = z.null();

export const bscVaultChainInitializedEvent = defineEvent(
  'BscVault.ChainInitialized',
  bscVaultChainInitialized,
);
