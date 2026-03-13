import { z } from 'zod';
import { palletCfBscIngressEgressPalletConfigUpdateBsc } from '../common';

export const bscIngressEgressPalletConfigUpdated = z.object({
  update: palletCfBscIngressEgressPalletConfigUpdateBsc,
});
