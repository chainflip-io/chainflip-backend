import { z } from 'zod';
import { palletCfPolkadotIngressEgressPalletConfigUpdatePolkadot } from '../common';

export const polkadotIngressEgressPalletConfigUpdated = z.object({
  update: palletCfPolkadotIngressEgressPalletConfigUpdatePolkadot,
});
