import { z } from 'zod';
import { palletCfChainTrackingPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinChainTrackingPalletConfigUpdated = z.object({
  update: palletCfChainTrackingPalletConfigUpdate,
});

export const bitcoinChainTrackingPalletConfigUpdatedEvent = defineEvent(
  'BitcoinChainTracking.PalletConfigUpdated',
  bitcoinChainTrackingPalletConfigUpdated,
);
