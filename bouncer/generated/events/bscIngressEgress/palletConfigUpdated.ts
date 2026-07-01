import { z } from 'zod';
import { palletCfBscIngressEgressPalletConfigUpdateBsc } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressPalletConfigUpdated = z.object({
  update: palletCfBscIngressEgressPalletConfigUpdateBsc,
});

export const bscIngressEgressPalletConfigUpdatedEvent = defineEvent(
  'BscIngressEgress.PalletConfigUpdated',
  bscIngressEgressPalletConfigUpdated,
);
