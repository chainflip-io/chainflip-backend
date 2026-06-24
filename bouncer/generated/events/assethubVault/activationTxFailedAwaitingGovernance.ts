import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: hexString,
});

export const assethubVaultActivationTxFailedAwaitingGovernanceEvent = defineEvent(
  'AssethubVault.ActivationTxFailedAwaitingGovernance',
  assethubVaultActivationTxFailedAwaitingGovernance,
);
