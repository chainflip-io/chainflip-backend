import { z } from 'zod';
import { cfChainsEvmTransaction } from '../common';

export const ethereumBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsEvmTransaction,
});
