import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: hexString,
});

export const solanaVaultActivationTxFailedAwaitingGovernanceEvent = defineEvent(
  'SolanaVault.ActivationTxFailedAwaitingGovernance',
  solanaVaultActivationTxFailedAwaitingGovernance,
);
