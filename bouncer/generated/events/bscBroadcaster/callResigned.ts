import { z } from 'zod';
import { cfChainsEvmTransaction } from '../common';

export const bscBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsEvmTransaction,
});
