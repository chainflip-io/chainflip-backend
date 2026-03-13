import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfChainsEvmDepositDetails,
  cfPrimitivesChainsAssetsBscAsset,
  hexString,
  numberOrHex,
  palletCfBscIngressEgressDepositAction,
} from '../common';

export const bscIngressEgressDepositFinalised = z.object({
  depositAddress: hexString.nullish(),
  asset: cfPrimitivesChainsAssetsBscAsset,
  amount: numberOrHex,
  blockHeight: numberOrHex,
  depositDetails: cfChainsEvmDepositDetails,
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  action: palletCfBscIngressEgressDepositAction,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});
