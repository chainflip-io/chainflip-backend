import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';

export const tronIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: cfChainsEvmDepositDetails,
});
