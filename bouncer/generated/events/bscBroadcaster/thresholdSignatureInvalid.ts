import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });

export const bscBroadcasterThresholdSignatureInvalidEvent = defineEvent(
  'BscBroadcaster.ThresholdSignatureInvalid',
  bscBroadcasterThresholdSignatureInvalid,
);
