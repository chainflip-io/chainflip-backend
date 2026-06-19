import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });

export const bitcoinBroadcasterThresholdSignatureInvalidEvent = defineEvent(
  'BitcoinBroadcaster.ThresholdSignatureInvalid',
  bitcoinBroadcasterThresholdSignatureInvalid,
);
