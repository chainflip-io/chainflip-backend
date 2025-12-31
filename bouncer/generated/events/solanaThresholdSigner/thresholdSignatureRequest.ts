import { z } from 'zod';
import { accountId, hexString, numberOrHex, solPrimTransactionVersionedMessage } from '../common';

export const solanaThresholdSignerThresholdSignatureRequest = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  epoch: z.number(),
  key: hexString,
  signatories: z.array(accountId),
  payload: solPrimTransactionVersionedMessage,
});
