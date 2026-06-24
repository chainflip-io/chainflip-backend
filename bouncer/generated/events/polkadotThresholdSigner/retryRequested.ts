import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerRetryRequested = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
});

export const polkadotThresholdSignerRetryRequestedEvent = defineEvent(
  'PolkadotThresholdSigner.RetryRequested',
  polkadotThresholdSignerRetryRequested,
);
