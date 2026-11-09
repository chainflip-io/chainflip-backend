import { z } from 'zod';
import {
  accountId,
  cfTraitsFailedDepositRecoveryOutcome,
  hexString,
  palletCfAssethubIngressEgressFailedVaultDepositsFailedVaultDepositInfoAssethub,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressFailedDepositRecovered = z.object({
  queueId: hexString.nullish(),
  deposit: palletCfAssethubIngressEgressFailedVaultDepositsFailedVaultDepositInfoAssethub,
  brokerId: accountId,
  recoveryAccount: accountId,
  outcome: cfTraitsFailedDepositRecoveryOutcome,
});

export const assethubIngressEgressFailedDepositRecoveredEvent = defineEvent(
  'AssethubIngressEgress.FailedDepositRecovered',
  assethubIngressEgressFailedDepositRecovered,
);
