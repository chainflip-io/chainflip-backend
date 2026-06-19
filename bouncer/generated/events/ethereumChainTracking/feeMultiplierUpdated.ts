import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumChainTrackingFeeMultiplierUpdated = z.object({
  newFeeMultiplier: numberOrHex,
});

export const ethereumChainTrackingFeeMultiplierUpdatedEvent = defineEvent(
  'EthereumChainTracking.FeeMultiplierUpdated',
  ethereumChainTrackingFeeMultiplierUpdated,
);
