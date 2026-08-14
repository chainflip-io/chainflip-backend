import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsHubAsset,
  cfPrimitivesTxId,
  hexString,
  numberOrHex,
  palletCfAssethubIngressEgressDepositAction,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressDepositFinalised = z.object({
  depositAddress: hexString.nullish(),
  asset: cfPrimitivesChainsAssetsHubAsset,
  amount: numberOrHex,
  blockHeight: z.number(),
  depositDetails: cfPrimitivesTxId,
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  action: palletCfAssethubIngressEgressDepositAction,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});

export const assethubIngressEgressDepositFinalisedEvent = defineEvent(
  'AssethubIngressEgress.DepositFinalised',
  assethubIngressEgressDepositFinalised,
);
