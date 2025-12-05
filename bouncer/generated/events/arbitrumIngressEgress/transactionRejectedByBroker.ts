import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';

export const arbitrumIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: cfChainsEvmDepositDetails,
});
