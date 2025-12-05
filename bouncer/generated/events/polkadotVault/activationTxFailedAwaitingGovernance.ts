import { z } from 'zod';
import { hexString } from '../common';

export const polkadotVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: hexString,
});
