import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';

export const bscVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: cfChainsEvmAggKey,
});
