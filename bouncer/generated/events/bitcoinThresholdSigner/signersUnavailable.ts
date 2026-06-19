import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerSignersUnavailable = z.object({
  requestId: z.number(),
  attemptCount: z.number(),
});

export const bitcoinThresholdSignerSignersUnavailableEvent = defineEvent(
  'BitcoinThresholdSigner.SignersUnavailable',
  bitcoinThresholdSignerSignersUnavailable,
);
