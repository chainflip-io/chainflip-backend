import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumChainTrackingFeeMultiplierUpdated = z.object({
  newFeeMultiplier: numberOrHex,
});

export const arbitrumChainTrackingFeeMultiplierUpdatedEvent = defineEvent(
  'ArbitrumChainTracking.FeeMultiplierUpdated',
  arbitrumChainTrackingFeeMultiplierUpdated,
);
