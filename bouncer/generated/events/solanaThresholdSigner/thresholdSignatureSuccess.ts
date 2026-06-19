import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerThresholdSignatureSuccess = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
});

export const solanaThresholdSignerThresholdSignatureSuccessEvent = defineEvent(
  'SolanaThresholdSigner.ThresholdSignatureSuccess',
  solanaThresholdSignerThresholdSignatureSuccess,
);
