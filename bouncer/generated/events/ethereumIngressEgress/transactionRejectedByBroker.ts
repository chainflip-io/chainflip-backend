import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';

export const ethereumIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: cfChainsEvmDepositDetails,
});
