import { z } from 'zod';
import { cfPrimitivesChainsAssetsTronAsset, hexString, numberOrHex } from '../common';

export const tronIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsTronAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
});
