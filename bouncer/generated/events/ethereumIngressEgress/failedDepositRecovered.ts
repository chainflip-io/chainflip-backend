import { z } from 'zod';
import {
  accountId,
  cfTraitsFailedDepositRecoveryOutcome,
  hexString,
  palletCfEthereumIngressEgressFailedVaultDepositsFailedVaultDepositInfoEthereum,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressFailedDepositRecovered = z.object({
  queueId: hexString.nullish(),
  deposit: palletCfEthereumIngressEgressFailedVaultDepositsFailedVaultDepositInfoEthereum,
  brokerId: accountId,
  recoveryAccount: accountId,
  outcome: cfTraitsFailedDepositRecoveryOutcome,
});

export const ethereumIngressEgressFailedDepositRecoveredEvent = defineEvent(
  'EthereumIngressEgress.FailedDepositRecovered',
  ethereumIngressEgressFailedDepositRecovered,
);
