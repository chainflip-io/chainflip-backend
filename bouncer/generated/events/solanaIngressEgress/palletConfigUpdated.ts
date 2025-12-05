import { z } from 'zod';
import { palletCfSolanaIngressEgressPalletConfigUpdateSolana } from '../common';

export const solanaIngressEgressPalletConfigUpdated = z.object({
  update: palletCfSolanaIngressEgressPalletConfigUpdateSolana,
});
