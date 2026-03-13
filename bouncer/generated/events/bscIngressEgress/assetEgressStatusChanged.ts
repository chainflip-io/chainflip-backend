import { z } from 'zod';
import { cfPrimitivesChainsAssetsBscAsset } from '../common';

export const bscIngressEgressAssetEgressStatusChanged = z.object({
  asset: cfPrimitivesChainsAssetsBscAsset,
  disabled: z.boolean(),
});
