import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsDotAsset,
  hexString,
  numberOrHex,
  palletCfPolkadotIngressEgressDepositAction,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressDepositFinalised = z.object({
  depositAddress: hexString.nullish(),
  asset: cfPrimitivesChainsAssetsDotAsset,
  amount: numberOrHex,
  blockHeight: z.number(),
  depositDetails: z.number(),
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  action: palletCfPolkadotIngressEgressDepositAction,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});

export const polkadotIngressEgressDepositFinalisedEvent = defineEvent(
  'PolkadotIngressEgress.DepositFinalised',
  polkadotIngressEgressDepositFinalised,
);
