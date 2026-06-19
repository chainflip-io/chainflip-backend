import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumVaultVaultActivationCompleted = z.null();

export const ethereumVaultVaultActivationCompletedEvent = defineEvent(
  'EthereumVault.VaultActivationCompleted',
  ethereumVaultVaultActivationCompleted,
);
