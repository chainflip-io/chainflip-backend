import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });

export const polkadotBroadcasterThresholdSignatureInvalidEvent = defineEvent(
  'PolkadotBroadcaster.ThresholdSignatureInvalid',
  polkadotBroadcasterThresholdSignatureInvalid,
);
