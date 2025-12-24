import { z } from 'zod';
import { accountId, hexString } from '../common';

export const bitcoinIngressEgressTransactionRejectionRequestReceived = z.object({
  accountId,
  txId: hexString,
  expiresAt: z.number(),
});
