import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaVaultChainInitialized = z.null();

export const solanaVaultChainInitializedEvent = defineEvent(
  'SolanaVault.ChainInitialized',
  solanaVaultChainInitialized,
);
