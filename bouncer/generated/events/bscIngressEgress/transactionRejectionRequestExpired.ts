import { z } from 'zod';
import { accountId, hexString } from '../common';

export const bscIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: hexString,
});
