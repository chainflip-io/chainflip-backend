import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerRetryRequested = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
});

export const bitcoinThresholdSignerRetryRequestedEvent = defineEvent(
  'BitcoinThresholdSigner.RetryRequested',
  bitcoinThresholdSignerRetryRequested,
);
