import { z } from 'zod';
import {
  cfChainsAddressForeignChainAddress,
  cfPrimitivesChainsForeignChain,
  spRuntimeDispatchError,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assetBalancesRefundSkipped = z.object({
  reason: spRuntimeDispatchError,
  chain: cfPrimitivesChainsForeignChain,
  address: cfChainsAddressForeignChainAddress,
});

export const assetBalancesRefundSkippedEvent = defineEvent(
  'AssetBalances.RefundSkipped',
  assetBalancesRefundSkipped,
);
