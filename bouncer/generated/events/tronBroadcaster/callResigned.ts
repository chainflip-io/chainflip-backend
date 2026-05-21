import { z } from 'zod';
import { cfChainsTronTronTransaction } from '../common';

export const tronBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsTronTronTransaction,
});
