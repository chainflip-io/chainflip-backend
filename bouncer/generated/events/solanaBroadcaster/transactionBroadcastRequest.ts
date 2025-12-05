import { z } from 'zod';
import { accountId, cfChainsSolSolanaTransactionData, hexString } from '../common';

export const solanaBroadcasterTransactionBroadcastRequest = z.object({
  broadcastId: z.number(),
  nominee: accountId,
  transactionPayload: cfChainsSolSolanaTransactionData,
  transactionOutId: hexString,
});
