import { z } from 'zod';
import { cfChainsBtcAggKey } from '../common';

export const bitcoinVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: cfChainsBtcAggKey,
});
