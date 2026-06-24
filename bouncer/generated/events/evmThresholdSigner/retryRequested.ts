import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerRetryRequested = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
});

export const evmThresholdSignerRetryRequestedEvent = defineEvent(
  'EvmThresholdSigner.RetryRequested',
  evmThresholdSignerRetryRequested,
);
