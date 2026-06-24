import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerSignersUnavailable = z.object({
  requestId: z.number(),
  attemptCount: z.number(),
});

export const polkadotThresholdSignerSignersUnavailableEvent = defineEvent(
  'PolkadotThresholdSigner.SignersUnavailable',
  polkadotThresholdSignerSignersUnavailable,
);
