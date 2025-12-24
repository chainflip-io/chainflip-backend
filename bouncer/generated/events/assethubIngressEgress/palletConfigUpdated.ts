import { z } from 'zod';
import { palletCfAssethubIngressEgressPalletConfigUpdateAssethub } from '../common';

export const assethubIngressEgressPalletConfigUpdated = z.object({
  update: palletCfAssethubIngressEgressPalletConfigUpdateAssethub,
});
