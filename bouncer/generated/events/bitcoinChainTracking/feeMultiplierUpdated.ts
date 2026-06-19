import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinChainTrackingFeeMultiplierUpdated = z.object({ newFeeMultiplier: numberOrHex });

export const bitcoinChainTrackingFeeMultiplierUpdatedEvent = defineEvent(
  'BitcoinChainTracking.FeeMultiplierUpdated',
  bitcoinChainTrackingFeeMultiplierUpdated,
);
