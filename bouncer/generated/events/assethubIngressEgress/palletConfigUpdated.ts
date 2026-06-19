import { z } from 'zod';
import { palletCfAssethubIngressEgressPalletConfigUpdateAssethub } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressPalletConfigUpdated = z.object({
  update: palletCfAssethubIngressEgressPalletConfigUpdateAssethub,
});

export const assethubIngressEgressPalletConfigUpdatedEvent = defineEvent(
  'AssethubIngressEgress.PalletConfigUpdated',
  assethubIngressEgressPalletConfigUpdated,
);
