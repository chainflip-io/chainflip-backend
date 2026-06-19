import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });

export const ethereumBroadcasterThresholdSignatureInvalidEvent = defineEvent(
  'EthereumBroadcaster.ThresholdSignatureInvalid',
  ethereumBroadcasterThresholdSignatureInvalid,
);
