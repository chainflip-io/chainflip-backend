import { z } from 'zod';
import { palletCfTronIngressEgressPalletConfigUpdateTron } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressPalletConfigUpdated = z.object({
  update: palletCfTronIngressEgressPalletConfigUpdateTron,
});

export const tronIngressEgressPalletConfigUpdatedEvent = defineEvent(
  'TronIngressEgress.PalletConfigUpdated',
  tronIngressEgressPalletConfigUpdated,
);
