import { z } from 'zod';
import { palletCfBitcoinIngressEgressPalletConfigUpdateBitcoin } from '../common';

export const bitcoinIngressEgressPalletConfigUpdated = z.object({
  update: palletCfBitcoinIngressEgressPalletConfigUpdateBitcoin,
});
