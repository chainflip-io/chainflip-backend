import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsTronAsset,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressInsufficientBoostLiquidity = z.object({
  prewitnessedDepositId: numberOrHex,
  asset: cfPrimitivesChainsAssetsTronAsset,
  amountAttempted: numberOrHex,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});

export const tronIngressEgressInsufficientBoostLiquidityEvent = defineEvent(
  'TronIngressEgress.InsufficientBoostLiquidity',
  tronIngressEgressInsufficientBoostLiquidity,
);
