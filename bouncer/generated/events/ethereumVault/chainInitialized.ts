import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumVaultChainInitialized = z.null();

export const ethereumVaultChainInitializedEvent = defineEvent(
  'EthereumVault.ChainInitialized',
  ethereumVaultChainInitialized,
);
