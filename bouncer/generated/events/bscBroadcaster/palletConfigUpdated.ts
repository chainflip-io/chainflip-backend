import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});

export const bscBroadcasterPalletConfigUpdatedEvent = defineEvent(
  'BscBroadcaster.PalletConfigUpdated',
  bscBroadcasterPalletConfigUpdated,
);
