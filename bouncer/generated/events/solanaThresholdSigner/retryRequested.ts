import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerRetryRequested = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
});

export const solanaThresholdSignerRetryRequestedEvent = defineEvent(
  'SolanaThresholdSigner.RetryRequested',
  solanaThresholdSignerRetryRequested,
);
