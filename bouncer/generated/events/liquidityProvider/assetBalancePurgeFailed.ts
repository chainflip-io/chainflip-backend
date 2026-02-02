import { z } from 'zod';
import {
  accountId,
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
  spRuntimeDispatchError,
} from '../common';

export const liquidityProviderAssetBalancePurgeFailed = z.object({
  accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
  error: spRuntimeDispatchError,
});
