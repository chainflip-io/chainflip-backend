import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});

export const assethubBroadcasterPalletConfigUpdatedEvent = defineEvent(
  'AssethubBroadcaster.PalletConfigUpdated',
  assethubBroadcasterPalletConfigUpdated,
);
