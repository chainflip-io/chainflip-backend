import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });

export const tronBroadcasterThresholdSignatureInvalidEvent = defineEvent(
  'TronBroadcaster.ThresholdSignatureInvalid',
  tronBroadcasterThresholdSignatureInvalid,
);
