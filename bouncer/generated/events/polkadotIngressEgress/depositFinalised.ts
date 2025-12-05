import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsDotAsset,
  hexString,
  numberOrHex,
  palletCfPolkadotIngressEgressDepositAction,
} from '../common';

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
