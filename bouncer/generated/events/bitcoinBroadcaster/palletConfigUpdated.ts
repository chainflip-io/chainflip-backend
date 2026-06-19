import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});

export const bitcoinBroadcasterPalletConfigUpdatedEvent = defineEvent(
  'BitcoinBroadcaster.PalletConfigUpdated',
  bitcoinBroadcasterPalletConfigUpdated,
);
