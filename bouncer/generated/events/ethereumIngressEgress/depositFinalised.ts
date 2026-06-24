import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfChainsEvmDepositDetails,
  cfPrimitivesChainsAssetsEthAsset,
  hexString,
  numberOrHex,
  palletCfEthereumIngressEgressDepositAction,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

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

export const ethereumIngressEgressDepositFinalisedEvent = defineEvent(
  'EthereumIngressEgress.DepositFinalised',
  ethereumIngressEgressDepositFinalised,
);
