import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerSignersUnavailable = z.object({
  requestId: z.number(),
  attemptCount: z.number(),
});

export const evmThresholdSignerSignersUnavailableEvent = defineEvent(
  'EvmThresholdSigner.SignersUnavailable',
  evmThresholdSignerSignersUnavailable,
);
