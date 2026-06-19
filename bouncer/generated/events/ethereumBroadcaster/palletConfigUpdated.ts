import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});

export const ethereumBroadcasterPalletConfigUpdatedEvent = defineEvent(
  'EthereumBroadcaster.PalletConfigUpdated',
  ethereumBroadcasterPalletConfigUpdated,
);
