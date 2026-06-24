import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubVaultChainInitialized = z.null();

export const assethubVaultChainInitializedEvent = defineEvent(
  'AssethubVault.ChainInitialized',
  assethubVaultChainInitialized,
);
