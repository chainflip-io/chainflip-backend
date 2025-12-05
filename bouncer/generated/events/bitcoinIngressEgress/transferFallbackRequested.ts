import { z } from 'zod';
import {
  cfChainsBtcScriptPubkey,
  cfPrimitivesChainsAssetsBtcAsset,
  cfTraitsScheduledEgressDetailsBitcoin,
  numberOrHex,
} from '../common';

export const bitcoinIngressEgressTransferFallbackRequested = z.object({
  asset: cfPrimitivesChainsAssetsBtcAsset,
  amount: numberOrHex,
  destinationAddress: cfChainsBtcScriptPubkey,
  broadcastId: z.number(),
  egressDetails: cfTraitsScheduledEgressDetailsBitcoin.nullish(),
});
