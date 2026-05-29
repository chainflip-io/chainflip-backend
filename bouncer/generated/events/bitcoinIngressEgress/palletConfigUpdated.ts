import { z } from 'zod';
import { palletCfBitcoinIngressEgressPalletConfigUpdateBitcoin } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressPalletConfigUpdated = z.object({
  update: palletCfBitcoinIngressEgressPalletConfigUpdateBitcoin,
});

export const bitcoinIngressEgressPalletConfigUpdatedEvent = defineEvent(
  'BitcoinIngressEgress.PalletConfigUpdated',
  bitcoinIngressEgressPalletConfigUpdated,
);
