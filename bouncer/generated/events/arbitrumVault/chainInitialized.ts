import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumVaultChainInitialized = z.null();

export const arbitrumVaultChainInitializedEvent = defineEvent(
  'ArbitrumVault.ChainInitialized',
  arbitrumVaultChainInitialized,
);
