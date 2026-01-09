import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfChainsSolVaultSwapOrDepositChannelId,
  cfPrimitivesChainsAssetsSolAsset,
  hexString,
  numberOrHex,
  palletCfSolanaIngressEgressDepositAction,
} from '../common';

export const solanaIngressEgressDepositBoosted = z.object({
  depositAddress: hexString.nullish(),
  asset: cfPrimitivesChainsAssetsSolAsset,
  amounts: z.array(z.tuple([z.number(), numberOrHex])),
  depositDetails: cfChainsSolVaultSwapOrDepositChannelId,
  prewitnessedDepositId: numberOrHex,
  channelId: numberOrHex.nullish(),
  blockHeight: numberOrHex,
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  boostFee: numberOrHex,
  action: palletCfSolanaIngressEgressDepositAction,
  originType: cfChainsDepositOriginType,
});
