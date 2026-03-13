import { z } from 'zod';
import { cfPrimitivesChainsAssetsBscAsset, hexString, numberOrHex } from '../common';

export const bscIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsBscAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
});
