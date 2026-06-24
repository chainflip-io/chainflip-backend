import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});

export const polkadotBroadcasterPalletConfigUpdatedEvent = defineEvent(
  'PolkadotBroadcaster.PalletConfigUpdated',
  polkadotBroadcasterPalletConfigUpdated,
);
