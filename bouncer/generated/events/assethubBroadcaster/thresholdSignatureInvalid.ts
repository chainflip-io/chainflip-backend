import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });

export const assethubBroadcasterThresholdSignatureInvalidEvent = defineEvent(
  'AssethubBroadcaster.ThresholdSignatureInvalid',
  assethubBroadcasterThresholdSignatureInvalid,
);
