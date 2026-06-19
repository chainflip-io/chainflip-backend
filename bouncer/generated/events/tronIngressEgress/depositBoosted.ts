import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfChainsEvmDepositDetails,
  cfPrimitivesChainsAssetsTronAsset,
  cfTraitsLendingBoostSource,
  hexString,
  numberOrHex,
  palletCfTronIngressEgressDepositAction,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressDepositBoosted = z.object({
  depositAddress: hexString.nullish(),
  asset: cfPrimitivesChainsAssetsTronAsset,
  amounts: z.array(z.tuple([cfTraitsLendingBoostSource, numberOrHex])),
  depositDetails: cfChainsEvmDepositDetails,
  prewitnessedDepositId: numberOrHex,
  channelId: numberOrHex.nullish(),
  blockHeight: numberOrHex,
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  boostFee: z.array(z.tuple([cfTraitsLendingBoostSource, numberOrHex])),
  action: palletCfTronIngressEgressDepositAction,
  originType: cfChainsDepositOriginType,
});

export const tronIngressEgressDepositBoostedEvent = defineEvent(
  'TronIngressEgress.DepositBoosted',
  tronIngressEgressDepositBoosted,
);
