import { z } from 'zod';
import { palletCfArbitrumIngressEgressPalletConfigUpdateArbitrum } from '../common';

export const arbitrumIngressEgressPalletConfigUpdated = z.object({
  update: palletCfArbitrumIngressEgressPalletConfigUpdateArbitrum,
});
