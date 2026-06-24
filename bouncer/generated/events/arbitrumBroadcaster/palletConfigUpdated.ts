import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});

export const arbitrumBroadcasterPalletConfigUpdatedEvent = defineEvent(
  'ArbitrumBroadcaster.PalletConfigUpdated',
  arbitrumBroadcasterPalletConfigUpdated,
);
