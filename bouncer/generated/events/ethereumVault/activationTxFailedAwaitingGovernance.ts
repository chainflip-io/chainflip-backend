import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';

export const ethereumVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: cfChainsEvmAggKey,
});
