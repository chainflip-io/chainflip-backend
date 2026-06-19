import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubChainTrackingFeeMultiplierUpdated = z.object({
  newFeeMultiplier: numberOrHex,
});

export const assethubChainTrackingFeeMultiplierUpdatedEvent = defineEvent(
  'AssethubChainTracking.FeeMultiplierUpdated',
  assethubChainTrackingFeeMultiplierUpdated,
);
