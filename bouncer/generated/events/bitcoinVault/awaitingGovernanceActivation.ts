import { z } from 'zod';
import { cfChainsBtcAggKey } from '../common';

export const bitcoinVaultAwaitingGovernanceActivation = z.object({
  newPublicKey: cfChainsBtcAggKey,
});
