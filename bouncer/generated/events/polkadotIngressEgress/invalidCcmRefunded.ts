import { z } from 'zod';
import { cfPrimitivesChainsAssetsDotAsset, hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressInvalidCcmRefunded = z.object({
  asset: cfPrimitivesChainsAssetsDotAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
});

export const polkadotIngressEgressInvalidCcmRefundedEvent = defineEvent(
  'PolkadotIngressEgress.InvalidCcmRefunded',
  polkadotIngressEgressInvalidCcmRefunded,
);
