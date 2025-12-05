import { z } from 'zod';
import { hexString } from '../common';

export const solanaVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: hexString,
});
