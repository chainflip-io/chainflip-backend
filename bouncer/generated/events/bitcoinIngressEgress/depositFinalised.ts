import { z } from 'zod';
import {
  cfChainsBtcScriptPubkey,
  cfChainsBtcUtxo,
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsBtcAsset,
  numberOrHex,
  palletCfBitcoinIngressEgressDepositAction,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressDepositFinalised = z.object({
  depositAddress: cfChainsBtcScriptPubkey.nullish(),
  asset: cfPrimitivesChainsAssetsBtcAsset,
  amount: numberOrHex,
  blockHeight: numberOrHex,
  depositDetails: cfChainsBtcUtxo,
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  action: palletCfBitcoinIngressEgressDepositAction,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});

export const bitcoinIngressEgressDepositFinalisedEvent = defineEvent(
  'BitcoinIngressEgress.DepositFinalised',
  bitcoinIngressEgressDepositFinalised,
);
