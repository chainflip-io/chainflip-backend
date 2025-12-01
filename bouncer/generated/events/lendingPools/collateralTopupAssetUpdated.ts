import { z } from 'zod';
import { accountId, cfPrimitivesChainsAssetsAnyAsset } from '../common';

export const lendingPoolsCollateralTopupAssetUpdated = z.object({
  borrowerId: accountId,
  collateralTopupAsset: cfPrimitivesChainsAssetsAnyAsset.nullish(),
});
