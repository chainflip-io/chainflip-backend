import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsDotAsset,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressInsufficientBoostLiquidity = z.object({
  prewitnessedDepositId: numberOrHex,
  asset: cfPrimitivesChainsAssetsDotAsset,
  amountAttempted: numberOrHex,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});

export const polkadotIngressEgressInsufficientBoostLiquidityEvent = defineEvent(
  'PolkadotIngressEgress.InsufficientBoostLiquidity',
  polkadotIngressEgressInsufficientBoostLiquidity,
);
