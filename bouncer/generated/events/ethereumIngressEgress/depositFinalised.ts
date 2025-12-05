import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfChainsEvmDepositDetails,
  cfPrimitivesChainsAssetsEthAsset,
  hexString,
  numberOrHex,
  palletCfEthereumIngressEgressDepositAction,
} from '../common';

export const ethereumIngressEgressDepositFinalised = z.object({
  depositAddress: hexString.nullish(),
  asset: cfPrimitivesChainsAssetsEthAsset,
  amount: numberOrHex,
  blockHeight: numberOrHex,
  depositDetails: cfChainsEvmDepositDetails,
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  action: palletCfEthereumIngressEgressDepositAction,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});
