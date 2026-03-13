import { z } from 'zod';
import {
  numberOrHex,
  palletCfBscIngressEgressDepositFailedDetailsBsc,
  palletCfBscIngressEgressDepositFailedReason,
} from '../common';

export const bscIngressEgressDepositFailed = z.object({
  blockHeight: numberOrHex,
  reason: palletCfBscIngressEgressDepositFailedReason,
  details: palletCfBscIngressEgressDepositFailedDetailsBsc,
});
