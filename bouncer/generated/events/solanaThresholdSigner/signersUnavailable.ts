import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerSignersUnavailable = z.object({
  requestId: z.number(),
  attemptCount: z.number(),
});

export const solanaThresholdSignerSignersUnavailableEvent = defineEvent(
  'SolanaThresholdSigner.SignersUnavailable',
  solanaThresholdSignerSignersUnavailable,
);
