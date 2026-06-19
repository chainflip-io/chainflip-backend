import { z } from 'zod';
import { palletCfSolanaIngressEgressPalletConfigUpdateSolana } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressPalletConfigUpdated = z.object({
  update: palletCfSolanaIngressEgressPalletConfigUpdateSolana,
});

export const solanaIngressEgressPalletConfigUpdatedEvent = defineEvent(
  'SolanaIngressEgress.PalletConfigUpdated',
  solanaIngressEgressPalletConfigUpdated,
);
