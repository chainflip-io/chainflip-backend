import { z } from 'zod';
import { accountId, hexString, numberOrHex, solPrimTransactionVersionedMessage } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerThresholdSignatureRequest = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  epoch: z.number(),
  key: hexString,
  signatories: z.array(accountId),
  payload: solPrimTransactionVersionedMessage,
});

export const solanaThresholdSignerThresholdSignatureRequestEvent = defineEvent(
  'SolanaThresholdSigner.ThresholdSignatureRequest',
  solanaThresholdSignerThresholdSignatureRequest,
);
