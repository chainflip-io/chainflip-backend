import { z } from 'zod';
import { palletCfChainTrackingPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotChainTrackingPalletConfigUpdated = z.object({
  update: palletCfChainTrackingPalletConfigUpdate,
});

export const polkadotChainTrackingPalletConfigUpdatedEvent = defineEvent(
  'PolkadotChainTracking.PalletConfigUpdated',
  polkadotChainTrackingPalletConfigUpdated,
);
