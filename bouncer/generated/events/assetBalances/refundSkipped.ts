import { z } from 'zod';
import {
  cfChainsAddressForeignChainAddress,
  cfPrimitivesChainsForeignChain,
  spRuntimeDispatchError,
} from '../common';

export const assetBalancesRefundSkipped = z.object({
  reason: spRuntimeDispatchError,
  chain: cfPrimitivesChainsForeignChain,
  address: cfChainsAddressForeignChainAddress,
});
