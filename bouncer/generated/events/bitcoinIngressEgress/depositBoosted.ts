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
import { defineEvent } from '@chainflip/processor/event';

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
  boostFee: z.array(z.tuple([cfTraitsLendingBoostSource, numberOrHex])),
  action: palletCfBitcoinIngressEgressDepositAction,
  originType: cfChainsDepositOriginType,
});

export const bitcoinIngressEgressDepositBoostedEvent = defineEvent(
  'BitcoinIngressEgress.DepositBoosted',
  bitcoinIngressEgressDepositBoosted,
);
