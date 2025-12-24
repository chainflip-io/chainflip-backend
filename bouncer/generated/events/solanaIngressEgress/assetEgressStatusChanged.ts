import { z } from 'zod';
import { cfPrimitivesChainsAssetsSolAsset } from '../common';

export const solanaIngressEgressAssetEgressStatusChanged = z.object({
  asset: cfPrimitivesChainsAssetsSolAsset,
  disabled: z.boolean(),
});
