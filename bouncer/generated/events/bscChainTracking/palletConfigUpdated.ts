import { z } from 'zod';
import { palletCfChainTrackingPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscChainTrackingPalletConfigUpdated = z.object({
  update: palletCfChainTrackingPalletConfigUpdate,
});

export const bscChainTrackingPalletConfigUpdatedEvent = defineEvent(
  'BscChainTracking.PalletConfigUpdated',
  bscChainTrackingPalletConfigUpdated,
);
