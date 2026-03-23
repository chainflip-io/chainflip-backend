import { z } from 'zod';
import {
  cfChainsBtcScriptPubkey,
  cfChainsBtcUtxo,
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsBtcAsset,
  cfTraitsLendingBoostSource,
  numberOrHex,
  palletCfBitcoinIngressEgressDepositAction,
} from '../common';

export const bitcoinIngressEgressDepositBoosted = z.object({
  depositAddress: cfChainsBtcScriptPubkey.nullish(),
  asset: cfPrimitivesChainsAssetsBtcAsset,
  amounts: z.array(z.tuple([cfTraitsLendingBoostSource, numberOrHex])),
  depositDetails: cfChainsBtcUtxo,
  prewitnessedDepositId: numberOrHex,
  channelId: numberOrHex.nullish(),
  blockHeight: numberOrHex,
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  boostFee: numberOrHex,
  action: palletCfBitcoinIngressEgressDepositAction,
  originType: cfChainsDepositOriginType,
});
