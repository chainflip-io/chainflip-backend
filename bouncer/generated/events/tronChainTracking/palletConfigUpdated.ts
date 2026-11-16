import { z } from 'zod';
import { palletCfChainTrackingPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronChainTrackingPalletConfigUpdated = z.object({
  update: palletCfChainTrackingPalletConfigUpdate,
});

export const tronChainTrackingPalletConfigUpdatedEvent = defineEvent(
  'TronChainTracking.PalletConfigUpdated',
  tronChainTrackingPalletConfigUpdated,
);
