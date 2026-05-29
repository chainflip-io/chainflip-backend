import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });

export const arbitrumBroadcasterThresholdSignatureInvalidEvent = defineEvent(
  'ArbitrumBroadcaster.ThresholdSignatureInvalid',
  arbitrumBroadcasterThresholdSignatureInvalid,
);
