import { z } from 'zod';
import { palletCfEthereumIngressEgressPalletConfigUpdateEthereum } from '../common';

export const ethereumIngressEgressPalletConfigUpdated = z.object({
  update: palletCfEthereumIngressEgressPalletConfigUpdateEthereum,
});
