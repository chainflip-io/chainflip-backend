import { z } from 'zod';
import { cfPrimitivesChainsAssetsBscAsset, hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsBscAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
});

export const bscIngressEgressInvalidCcmRefundedEvent = defineEvent(
  'BscIngressEgress.InvalidCcmRefunded',
  bscIngressEgressInvalidCcmRefunded,
);
