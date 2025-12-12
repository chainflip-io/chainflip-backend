import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsHubAsset,
  hexString,
  numberOrHex,
  palletCfAssethubIngressEgressDepositAction,
} from '../common';

export const assethubIngressEgressDepositBoosted = z.object({
  depositAddress: hexString.nullish(),
  asset: cfPrimitivesChainsAssetsHubAsset,
  amounts: z.array(z.tuple([z.number(), numberOrHex])),
  depositDetails: z.number(),
  prewitnessedDepositId: numberOrHex,
  channelId: numberOrHex.nullish(),
  blockHeight: z.number(),
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  boostFee: numberOrHex,
  action: palletCfAssethubIngressEgressDepositAction,
  originType: cfChainsDepositOriginType,
});
