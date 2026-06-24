import { z } from 'zod';
import { cfPrimitivesChainsAssetsSolAsset, hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsSolAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
});

export const solanaIngressEgressInvalidCcmRefundedEvent = defineEvent(
  'SolanaIngressEgress.InvalidCcmRefunded',
  solanaIngressEgressInvalidCcmRefunded,
);
