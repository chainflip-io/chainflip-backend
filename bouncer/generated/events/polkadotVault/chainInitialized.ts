import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotVaultChainInitialized = z.null();

export const polkadotVaultChainInitializedEvent = defineEvent(
  'PolkadotVault.ChainInitialized',
  polkadotVaultChainInitialized,
);
