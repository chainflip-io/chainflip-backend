import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';

export const assetBalancesVaultDeficitDetected = z.object({
  chain: cfPrimitivesChainsForeignChain,
  amountOwed: numberOrHex,
  available: numberOrHex,
});
