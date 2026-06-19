import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsHubAsset,
  cfTraitsLendingBoostSource,
  hexString,
  numberOrHex,
  palletCfAssethubIngressEgressDepositAction,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressDepositBoosted = z.object({
  depositAddress: hexString.nullish(),
  asset: cfPrimitivesChainsAssetsHubAsset,
  amounts: z.array(z.tuple([cfTraitsLendingBoostSource, numberOrHex])),
  depositDetails: z.number(),
  prewitnessedDepositId: numberOrHex,
  channelId: numberOrHex.nullish(),
  blockHeight: z.number(),
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  boostFee: z.array(z.tuple([cfTraitsLendingBoostSource, numberOrHex])),
  action: palletCfAssethubIngressEgressDepositAction,
  originType: cfChainsDepositOriginType,
});

export const assethubIngressEgressDepositBoostedEvent = defineEvent(
  'AssethubIngressEgress.DepositBoosted',
  assethubIngressEgressDepositBoosted,
);
