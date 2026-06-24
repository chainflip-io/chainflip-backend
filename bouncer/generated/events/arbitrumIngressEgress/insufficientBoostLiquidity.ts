import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsArbAsset,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressInsufficientBoostLiquidity = z.object({
  prewitnessedDepositId: numberOrHex,
  asset: cfPrimitivesChainsAssetsArbAsset,
  amountAttempted: numberOrHex,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});

export const arbitrumIngressEgressInsufficientBoostLiquidityEvent = defineEvent(
  'ArbitrumIngressEgress.InsufficientBoostLiquidity',
  arbitrumIngressEgressInsufficientBoostLiquidity,
);
