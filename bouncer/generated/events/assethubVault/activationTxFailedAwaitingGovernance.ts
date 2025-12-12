import { z } from 'zod';
import { hexString } from '../common';

export const assethubVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: hexString,
});
