import { z } from 'zod';
import { accountId, cfChainsBtcBitcoinTransactionData, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinBroadcasterTransactionBroadcastRequest = z.object({
  broadcastId: z.number(),
  nominee: accountId,
  transactionPayload: cfChainsBtcBitcoinTransactionData,
  transactionOutId: hexString,
});

export const bitcoinBroadcasterTransactionBroadcastRequestEvent = defineEvent(
  'BitcoinBroadcaster.TransactionBroadcastRequest',
  bitcoinBroadcasterTransactionBroadcastRequest,
);
