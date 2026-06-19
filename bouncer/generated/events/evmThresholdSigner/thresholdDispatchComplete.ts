import { z } from 'zod';
import { dispatchResult, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerThresholdDispatchComplete = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  result: dispatchResult,
});

export const evmThresholdSignerThresholdDispatchCompleteEvent = defineEvent(
  'EvmThresholdSigner.ThresholdDispatchComplete',
  evmThresholdSignerThresholdDispatchComplete,
);
