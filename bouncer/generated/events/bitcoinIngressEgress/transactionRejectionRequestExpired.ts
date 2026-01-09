import { z } from 'zod';
import { accountId, hexString } from '../common';

export const bitcoinIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: hexString,
});
