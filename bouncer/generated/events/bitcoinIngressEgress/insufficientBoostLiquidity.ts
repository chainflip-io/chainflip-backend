import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsBtcAsset,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressInsufficientBoostLiquidity = z.object({
  prewitnessedDepositId: numberOrHex,
  asset: cfPrimitivesChainsAssetsBtcAsset,
  amountAttempted: numberOrHex,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});

export const bitcoinIngressEgressInsufficientBoostLiquidityEvent = defineEvent(
  'BitcoinIngressEgress.InsufficientBoostLiquidity',
  bitcoinIngressEgressInsufficientBoostLiquidity,
);
