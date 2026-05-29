import { z } from 'zod';
import { palletCfEthereumIngressEgressPalletConfigUpdateEthereum } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressPalletConfigUpdated = z.object({
  update: palletCfEthereumIngressEgressPalletConfigUpdateEthereum,
});

export const ethereumIngressEgressPalletConfigUpdatedEvent = defineEvent(
  'EthereumIngressEgress.PalletConfigUpdated',
  ethereumIngressEgressPalletConfigUpdated,
);
