import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerMaxRetriesReachedForRequest = z.object({ requestId: z.number() });

export const solanaThresholdSignerMaxRetriesReachedForRequestEvent = defineEvent(
  'SolanaThresholdSigner.MaxRetriesReachedForRequest',
  solanaThresholdSignerMaxRetriesReachedForRequest,
);
