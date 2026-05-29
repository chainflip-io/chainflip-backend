import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinVaultChainInitialized = z.null();

export const bitcoinVaultChainInitializedEvent = defineEvent(
  'BitcoinVault.ChainInitialized',
  bitcoinVaultChainInitialized,
);
