import { z } from 'zod';
import {
  cfChainsBtcScriptPubkey,
  cfChainsBtcUtxo,
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsBtcAsset,
  numberOrHex,
  palletCfBitcoinIngressEgressDepositAction,
} from '../common';

export const bitcoinIngressEgressDepositBoosted = z.object({
  depositAddress: cfChainsBtcScriptPubkey.nullish(),
  asset: cfPrimitivesChainsAssetsBtcAsset,
  amounts: z.array(z.tuple([z.number(), numberOrHex])),
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
