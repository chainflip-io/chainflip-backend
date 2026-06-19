import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });

export const solanaBroadcasterThresholdSignatureInvalidEvent = defineEvent(
  'SolanaBroadcaster.ThresholdSignatureInvalid',
  solanaBroadcasterThresholdSignatureInvalid,
);
