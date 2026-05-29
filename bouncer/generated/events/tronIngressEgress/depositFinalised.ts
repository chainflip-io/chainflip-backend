import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfChainsEvmDepositDetails,
  cfPrimitivesChainsAssetsTronAsset,
  hexString,
  numberOrHex,
  palletCfTronIngressEgressDepositAction,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressDepositFinalised = z.object({
  depositAddress: hexString.nullish(),
  asset: cfPrimitivesChainsAssetsTronAsset,
  amount: numberOrHex,
  blockHeight: numberOrHex,
  depositDetails: cfChainsEvmDepositDetails,
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  action: palletCfTronIngressEgressDepositAction,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});

export const tronIngressEgressDepositFinalisedEvent = defineEvent(
  'TronIngressEgress.DepositFinalised',
  tronIngressEgressDepositFinalised,
);
