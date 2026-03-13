import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';

export const tronVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: cfChainsEvmAggKey,
});
