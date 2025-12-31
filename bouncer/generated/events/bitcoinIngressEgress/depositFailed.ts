import { z } from 'zod';
import {
  numberOrHex,
  palletCfBitcoinIngressEgressDepositFailedDetailsBitcoin,
  palletCfBitcoinIngressEgressDepositFailedReason,
} from '../common';

export const bitcoinIngressEgressDepositFailed = z.object({
  blockHeight: numberOrHex,
  reason: palletCfBitcoinIngressEgressDepositFailedReason,
  details: palletCfBitcoinIngressEgressDepositFailedDetailsBitcoin,
});
