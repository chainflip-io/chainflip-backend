import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsArbAsset,
  numberOrHex,
} from '../common';

export const arbitrumIngressEgressInsufficientBoostLiquidity = z.object({
  prewitnessedDepositId: numberOrHex,
  asset: cfPrimitivesChainsAssetsArbAsset,
  amountAttempted: numberOrHex,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});
