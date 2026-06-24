import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfChainsEvmDepositDetails,
  cfPrimitivesChainsAssetsArbAsset,
  cfTraitsLendingBoostSource,
  hexString,
  numberOrHex,
  palletCfArbitrumIngressEgressDepositAction,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressDepositBoosted = z.object({
  depositAddress: hexString.nullish(),
  asset: cfPrimitivesChainsAssetsArbAsset,
  amounts: z.array(z.tuple([cfTraitsLendingBoostSource, numberOrHex])),
  depositDetails: cfChainsEvmDepositDetails,
  prewitnessedDepositId: numberOrHex,
  channelId: numberOrHex.nullish(),
  blockHeight: numberOrHex,
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  boostFee: z.array(z.tuple([cfTraitsLendingBoostSource, numberOrHex])),
  action: palletCfArbitrumIngressEgressDepositAction,
  originType: cfChainsDepositOriginType,
});

export const arbitrumIngressEgressDepositBoostedEvent = defineEvent(
  'ArbitrumIngressEgress.DepositBoosted',
  arbitrumIngressEgressDepositBoosted,
);
