import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});

export const tronBroadcasterPalletConfigUpdatedEvent = defineEvent(
  'TronBroadcaster.PalletConfigUpdated',
  tronBroadcasterPalletConfigUpdated,
);
