import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumVaultVaultActivationCompleted = z.null();

export const arbitrumVaultVaultActivationCompletedEvent = defineEvent(
  'ArbitrumVault.VaultActivationCompleted',
  arbitrumVaultVaultActivationCompleted,
);
