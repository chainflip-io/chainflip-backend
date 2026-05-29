import { z } from 'zod';
import { cfPrimitivesChainsAssetsTronAsset, hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsTronAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
});

export const tronIngressEgressInvalidCcmRefundedEvent = defineEvent(
  'TronIngressEgress.InvalidCcmRefunded',
  tronIngressEgressInvalidCcmRefunded,
);
