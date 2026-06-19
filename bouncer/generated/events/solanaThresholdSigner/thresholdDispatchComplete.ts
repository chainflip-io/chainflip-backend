import { z } from 'zod';
import { dispatchResult, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerThresholdDispatchComplete = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  result: dispatchResult,
});

export const solanaThresholdSignerThresholdDispatchCompleteEvent = defineEvent(
  'SolanaThresholdSigner.ThresholdDispatchComplete',
  solanaThresholdSignerThresholdDispatchComplete,
);
