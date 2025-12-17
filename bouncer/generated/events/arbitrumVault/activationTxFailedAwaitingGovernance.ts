import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';

export const arbitrumVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: cfChainsEvmAggKey,
});
