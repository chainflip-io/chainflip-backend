import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinVaultVaultActivationCompleted = z.null();

export const bitcoinVaultVaultActivationCompletedEvent = defineEvent(
  'BitcoinVault.VaultActivationCompleted',
  bitcoinVaultVaultActivationCompleted,
);
