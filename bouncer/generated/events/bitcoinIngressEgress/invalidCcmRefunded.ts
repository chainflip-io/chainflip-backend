import { z } from 'zod';
import { cfChainsBtcScriptPubkey, cfPrimitivesChainsAssetsBtcAsset, numberOrHex } from '../common';

export const bitcoinIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsBtcAsset,
  amount: numberOrHex,
  destinationAddress: cfChainsBtcScriptPubkey,
});
