import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerMaxRetriesReachedForRequest = z.object({
  requestId: z.number(),
});

export const polkadotThresholdSignerMaxRetriesReachedForRequestEvent = defineEvent(
  'PolkadotThresholdSigner.MaxRetriesReachedForRequest',
  polkadotThresholdSignerMaxRetriesReachedForRequest,
);
