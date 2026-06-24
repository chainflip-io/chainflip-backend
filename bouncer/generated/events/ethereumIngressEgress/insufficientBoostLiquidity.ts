import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsEthAsset,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressInsufficientBoostLiquidity = z.object({
  prewitnessedDepositId: numberOrHex,
  asset: cfPrimitivesChainsAssetsEthAsset,
  amountAttempted: numberOrHex,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});

export const ethereumIngressEgressInsufficientBoostLiquidityEvent = defineEvent(
  'EthereumIngressEgress.InsufficientBoostLiquidity',
  ethereumIngressEgressInsufficientBoostLiquidity,
);
