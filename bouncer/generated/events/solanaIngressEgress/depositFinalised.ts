import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfChainsSolVaultSwapOrDepositChannelId,
  cfPrimitivesChainsAssetsSolAsset,
  hexString,
  numberOrHex,
  palletCfSolanaIngressEgressDepositAction,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressDepositFinalised = z.object({
  depositAddress: hexString.nullish(),
  asset: cfPrimitivesChainsAssetsSolAsset,
  amount: numberOrHex,
  blockHeight: numberOrHex,
  depositDetails: cfChainsSolVaultSwapOrDepositChannelId,
  ingressFee: numberOrHex,
  maxBoostFeeBps: z.number(),
  action: palletCfSolanaIngressEgressDepositAction,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});

export const solanaIngressEgressDepositFinalisedEvent = defineEvent(
  'SolanaIngressEgress.DepositFinalised',
  solanaIngressEgressDepositFinalised,
);
