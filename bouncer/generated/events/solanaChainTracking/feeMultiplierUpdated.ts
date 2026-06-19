import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaChainTrackingFeeMultiplierUpdated = z.object({ newFeeMultiplier: numberOrHex });

export const solanaChainTrackingFeeMultiplierUpdatedEvent = defineEvent(
  'SolanaChainTracking.FeeMultiplierUpdated',
  solanaChainTrackingFeeMultiplierUpdated,
);
