import { z } from 'zod';
import { cfPrimitivesChainsAssetsArbAsset } from '../common';

export const arbitrumIngressEgressAssetEgressStatusChanged = z.object({
  asset: cfPrimitivesChainsAssetsArbAsset,
  disabled: z.boolean(),
});
