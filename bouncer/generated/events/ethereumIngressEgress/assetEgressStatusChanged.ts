import { z } from 'zod';
import { cfPrimitivesChainsAssetsEthAsset } from '../common';

export const ethereumIngressEgressAssetEgressStatusChanged = z.object({
  asset: cfPrimitivesChainsAssetsEthAsset,
  disabled: z.boolean(),
});
