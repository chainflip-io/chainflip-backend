import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsBscAsset,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressInsufficientBoostLiquidity = z.object({
  prewitnessedDepositId: numberOrHex,
  asset: cfPrimitivesChainsAssetsBscAsset,
  amountAttempted: numberOrHex,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});

export const bscIngressEgressInsufficientBoostLiquidityEvent = defineEvent(
  'BscIngressEgress.InsufficientBoostLiquidity',
  bscIngressEgressInsufficientBoostLiquidity,
);
