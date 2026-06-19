import { z } from 'zod';
import { palletCfArbitrumIngressEgressPalletConfigUpdateArbitrum } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressPalletConfigUpdated = z.object({
  update: palletCfArbitrumIngressEgressPalletConfigUpdateArbitrum,
});

export const arbitrumIngressEgressPalletConfigUpdatedEvent = defineEvent(
  'ArbitrumIngressEgress.PalletConfigUpdated',
  arbitrumIngressEgressPalletConfigUpdated,
);
