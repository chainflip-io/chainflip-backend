import { z } from 'zod';
import { dispatchResult, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerThresholdDispatchComplete = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  result: dispatchResult,
});

export const polkadotThresholdSignerThresholdDispatchCompleteEvent = defineEvent(
  'PolkadotThresholdSigner.ThresholdDispatchComplete',
  polkadotThresholdSignerThresholdDispatchComplete,
);
