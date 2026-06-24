import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerMaxRetriesReachedForRequest = z.object({ requestId: z.number() });

export const evmThresholdSignerMaxRetriesReachedForRequestEvent = defineEvent(
  'EvmThresholdSigner.MaxRetriesReachedForRequest',
  evmThresholdSignerMaxRetriesReachedForRequest,
);
