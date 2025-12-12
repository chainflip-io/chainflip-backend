import { z } from 'zod';
import { accountId, cfChainsBtcBitcoinTransactionData, hexString } from '../common';

export const bitcoinBroadcasterTransactionBroadcastRequest = z.object({
  broadcastId: z.number(),
  nominee: accountId,
  transactionPayload: cfChainsBtcBitcoinTransactionData,
  transactionOutId: hexString,
});
