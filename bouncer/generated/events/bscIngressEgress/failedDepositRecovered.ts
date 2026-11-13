import { z } from 'zod';
import {
  accountId,
  cfTraitsFailedDepositRecoveryOutcome,
  hexString,
  palletCfBscIngressEgressFailedVaultDepositsFailedVaultDepositInfoBsc,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressFailedDepositRecovered = z.object({
  queueId: hexString.nullish(),
  deposit: palletCfBscIngressEgressFailedVaultDepositsFailedVaultDepositInfoBsc,
  brokerId: accountId,
  recoveryAccount: accountId,
  outcome: cfTraitsFailedDepositRecoveryOutcome,
});

export const bscIngressEgressFailedDepositRecoveredEvent = defineEvent(
  'BscIngressEgress.FailedDepositRecovered',
  bscIngressEgressFailedDepositRecovered,
);
