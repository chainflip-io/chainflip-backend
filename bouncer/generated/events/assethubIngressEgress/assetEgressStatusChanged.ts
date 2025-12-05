import { z } from 'zod';
import { cfPrimitivesChainsAssetsHubAsset } from '../common';

export const assethubIngressEgressAssetEgressStatusChanged = z.object({
  asset: cfPrimitivesChainsAssetsHubAsset,
  disabled: z.boolean(),
});
