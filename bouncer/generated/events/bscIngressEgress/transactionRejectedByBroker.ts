import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';

export const bscIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: cfChainsEvmDepositDetails,
});
