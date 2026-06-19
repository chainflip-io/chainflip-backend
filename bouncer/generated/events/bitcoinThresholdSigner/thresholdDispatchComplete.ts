import { z } from 'zod';
import { dispatchResult, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerThresholdDispatchComplete = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  result: dispatchResult,
});

export const bitcoinThresholdSignerThresholdDispatchCompleteEvent = defineEvent(
  'BitcoinThresholdSigner.ThresholdDispatchComplete',
  bitcoinThresholdSignerThresholdDispatchComplete,
);
