import { z } from 'zod';
import { palletCfChainTrackingPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumChainTrackingPalletConfigUpdated = z.object({
  update: palletCfChainTrackingPalletConfigUpdate,
});

export const arbitrumChainTrackingPalletConfigUpdatedEvent = defineEvent(
  'ArbitrumChainTracking.PalletConfigUpdated',
  arbitrumChainTrackingPalletConfigUpdated,
);
