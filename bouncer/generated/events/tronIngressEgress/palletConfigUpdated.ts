import { z } from 'zod';
import { palletCfTronIngressEgressPalletConfigUpdateTron } from '../common';

export const tronIngressEgressPalletConfigUpdated = z.object({
  update: palletCfTronIngressEgressPalletConfigUpdateTron,
});
