import { z } from 'zod';
import { cfPrimitivesChainsAssetsSolAsset, hexString, numberOrHex } from '../common';

export const solanaIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsSolAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
});
