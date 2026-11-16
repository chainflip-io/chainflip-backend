import { z } from 'zod';
import { palletCfChainTrackingPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubChainTrackingPalletConfigUpdated = z.object({
  update: palletCfChainTrackingPalletConfigUpdate,
});

export const assethubChainTrackingPalletConfigUpdatedEvent = defineEvent(
  'AssethubChainTracking.PalletConfigUpdated',
  assethubChainTrackingPalletConfigUpdated,
);
