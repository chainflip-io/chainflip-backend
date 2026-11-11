import { z } from 'zod';
import { palletCfChainTrackingPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumChainTrackingPalletConfigUpdated = z.object({
  update: palletCfChainTrackingPalletConfigUpdate,
});

export const ethereumChainTrackingPalletConfigUpdatedEvent = defineEvent(
  'EthereumChainTracking.PalletConfigUpdated',
  ethereumChainTrackingPalletConfigUpdated,
);
