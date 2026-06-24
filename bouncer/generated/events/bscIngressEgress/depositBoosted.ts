import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfChainsEvmDepositDetails,
  cfPrimitivesChainsAssetsBscAsset,
  cfTraitsLendingBoostSource,
  hexString,
  numberOrHex,
  palletCfBscIngressEgressDepositAction,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressDepositBoosted = z.object({
  depositAddress: hexString.nullish(),
  asset: cfPrimitivesChainsAssetsBscAsset,
  amounts: z.array(z.tuple([cfTraitsLendingBoostSource, numberOrHex])),
  depositDetails: cfChainsEvmDepositDetails,
  prewitnessedDepositId: numberOrHex,
  channelId: numberOrHex.nullish(),
  blockHeight: numberOrHex,
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  boostFee: z.array(z.tuple([cfTraitsLendingBoostSource, numberOrHex])),
  action: palletCfBscIngressEgressDepositAction,
  originType: cfChainsDepositOriginType,
});

export const bscIngressEgressDepositBoostedEvent = defineEvent(
  'BscIngressEgress.DepositBoosted',
  bscIngressEgressDepositBoosted,
);
