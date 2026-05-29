import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerThresholdSignatureSuccess = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
});

export const evmThresholdSignerThresholdSignatureSuccessEvent = defineEvent(
  'EvmThresholdSigner.ThresholdSignatureSuccess',
  evmThresholdSignerThresholdSignatureSuccess,
);
