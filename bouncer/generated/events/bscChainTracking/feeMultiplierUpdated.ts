import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscChainTrackingFeeMultiplierUpdated = z.object({ newFeeMultiplier: numberOrHex });

export const bscChainTrackingFeeMultiplierUpdatedEvent = defineEvent(
  'BscChainTracking.FeeMultiplierUpdated',
  bscChainTrackingFeeMultiplierUpdated,
);
