import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsHubAsset,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressInsufficientBoostLiquidity = z.object({
  prewitnessedDepositId: numberOrHex,
  asset: cfPrimitivesChainsAssetsHubAsset,
  amountAttempted: numberOrHex,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});

export const assethubIngressEgressInsufficientBoostLiquidityEvent = defineEvent(
  'AssethubIngressEgress.InsufficientBoostLiquidity',
  assethubIngressEgressInsufficientBoostLiquidity,
);
