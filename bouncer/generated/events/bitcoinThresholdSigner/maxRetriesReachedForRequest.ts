import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerMaxRetriesReachedForRequest = z.object({
  requestId: z.number(),
});

export const bitcoinThresholdSignerMaxRetriesReachedForRequestEvent = defineEvent(
  'BitcoinThresholdSigner.MaxRetriesReachedForRequest',
  bitcoinThresholdSignerMaxRetriesReachedForRequest,
);
