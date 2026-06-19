import { z } from 'zod';
import {
  cfChainsAddressForeignChainAddress,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assetBalancesRefundScheduled = z.object({
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  destination: cfChainsAddressForeignChainAddress,
  amount: numberOrHex,
});

export const assetBalancesRefundScheduledEvent = defineEvent(
  'AssetBalances.RefundScheduled',
  assetBalancesRefundScheduled,
);
