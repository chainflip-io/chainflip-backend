import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assetBalancesVaultDeficitDetected = z.object({
  chain: cfPrimitivesChainsForeignChain,
  amountOwed: numberOrHex,
  available: numberOrHex,
});

export const assetBalancesVaultDeficitDetectedEvent = defineEvent(
  'AssetBalances.VaultDeficitDetected',
  assetBalancesVaultDeficitDetected,
);
