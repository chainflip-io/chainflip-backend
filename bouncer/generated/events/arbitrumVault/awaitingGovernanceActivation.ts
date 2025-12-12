import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';

export const arbitrumVaultAwaitingGovernanceActivation = z.object({
  newPublicKey: cfChainsEvmAggKey,
});
