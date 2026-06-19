import { z } from 'zod';
import { cfChainsBtcScriptPubkey, cfPrimitivesChainsAssetsBtcAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsBtcAsset,
  amount: numberOrHex,
  destinationAddress: cfChainsBtcScriptPubkey,
});

export const bitcoinIngressEgressInvalidCcmRefundedEvent = defineEvent(
  'BitcoinIngressEgress.InvalidCcmRefunded',
  bitcoinIngressEgressInvalidCcmRefunded,
);
