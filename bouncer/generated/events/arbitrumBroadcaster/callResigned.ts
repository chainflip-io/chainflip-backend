import { z } from 'zod';
import { cfChainsEvmTransaction } from '../common';

export const arbitrumBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsEvmTransaction,
});
