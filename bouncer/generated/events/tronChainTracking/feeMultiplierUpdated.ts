import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronChainTrackingFeeMultiplierUpdated = z.object({ newFeeMultiplier: numberOrHex });

export const tronChainTrackingFeeMultiplierUpdatedEvent = defineEvent(
  'TronChainTracking.FeeMultiplierUpdated',
  tronChainTrackingFeeMultiplierUpdated,
);
