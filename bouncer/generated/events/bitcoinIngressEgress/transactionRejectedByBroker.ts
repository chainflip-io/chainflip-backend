import { z } from 'zod';
import { cfChainsBtcUtxo } from '../common';

export const bitcoinIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: cfChainsBtcUtxo,
});
