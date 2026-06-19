import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotChainTrackingFeeMultiplierUpdated = z.object({
  newFeeMultiplier: numberOrHex,
});

export const polkadotChainTrackingFeeMultiplierUpdatedEvent = defineEvent(
  'PolkadotChainTracking.FeeMultiplierUpdated',
  polkadotChainTrackingFeeMultiplierUpdated,
);
