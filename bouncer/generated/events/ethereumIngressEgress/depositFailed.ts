import { z } from 'zod';
import {
  numberOrHex,
  palletCfEthereumIngressEgressDepositFailedDetailsEthereum,
  palletCfEthereumIngressEgressDepositFailedReason,
} from '../common';

export const ethereumIngressEgressDepositFailed = z.object({
  blockHeight: numberOrHex,
  reason: palletCfEthereumIngressEgressDepositFailedReason,
  details: palletCfEthereumIngressEgressDepositFailedDetailsEthereum,
});
