import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfChainsEvmDepositDetails,
  cfPrimitivesChainsAssetsEthAsset,
  cfTraitsLendingBoostSource,
  hexString,
  numberOrHex,
  palletCfEthereumIngressEgressDepositAction,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressDepositBoosted = z.object({
  depositAddress: hexString.nullish(),
  asset: cfPrimitivesChainsAssetsEthAsset,
  amounts: z.array(z.tuple([cfTraitsLendingBoostSource, numberOrHex])),
  depositDetails: cfChainsEvmDepositDetails,
  prewitnessedDepositId: numberOrHex,
  channelId: numberOrHex.nullish(),
  blockHeight: numberOrHex,
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  boostFee: z.array(z.tuple([cfTraitsLendingBoostSource, numberOrHex])),
  action: palletCfEthereumIngressEgressDepositAction,
  originType: cfChainsDepositOriginType,
});

export const ethereumIngressEgressDepositBoostedEvent = defineEvent(
  'EthereumIngressEgress.DepositBoosted',
  ethereumIngressEgressDepositBoosted,
);
