import { z } from 'zod';
import { palletCfPolkadotIngressEgressPalletConfigUpdatePolkadot } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressPalletConfigUpdated = z.object({
  update: palletCfPolkadotIngressEgressPalletConfigUpdatePolkadot,
});

export const polkadotIngressEgressPalletConfigUpdatedEvent = defineEvent(
  'PolkadotIngressEgress.PalletConfigUpdated',
  polkadotIngressEgressPalletConfigUpdated,
);
