import { z } from 'zod';
import { cfPrimitivesChainsAssetsTronAsset } from '../common';

export const tronIngressEgressAssetEgressStatusChanged = z.object({
  asset: cfPrimitivesChainsAssetsTronAsset,
  disabled: z.boolean(),
});
