import { z } from 'zod';
import {
  cfChainsBtcScriptPubkey,
  cfPrimitivesChainsAssetsBtcAsset,
  cfTraitsScheduledEgressDetailsBitcoin,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressTransferFallbackRequested = z.object({
  asset: cfPrimitivesChainsAssetsBtcAsset,
  amount: numberOrHex,
  destinationAddress: cfChainsBtcScriptPubkey,
  broadcastId: z.number(),
  egressDetails: cfTraitsScheduledEgressDetailsBitcoin.nullish(),
});

export const bitcoinIngressEgressTransferFallbackRequestedEvent = defineEvent(
  'BitcoinIngressEgress.TransferFallbackRequested',
  bitcoinIngressEgressTransferFallbackRequested,
);
