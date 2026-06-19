import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});

export const solanaBroadcasterPalletConfigUpdatedEvent = defineEvent(
  'SolanaBroadcaster.PalletConfigUpdated',
  solanaBroadcasterPalletConfigUpdated,
);
