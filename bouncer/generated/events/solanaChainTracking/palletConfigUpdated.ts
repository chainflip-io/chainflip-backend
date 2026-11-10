import { z } from 'zod';
import { palletCfChainTrackingPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaChainTrackingPalletConfigUpdated = z.object({
  update: palletCfChainTrackingPalletConfigUpdate,
});

export const solanaChainTrackingPalletConfigUpdatedEvent = defineEvent(
  'SolanaChainTracking.PalletConfigUpdated',
  solanaChainTrackingPalletConfigUpdated,
);
