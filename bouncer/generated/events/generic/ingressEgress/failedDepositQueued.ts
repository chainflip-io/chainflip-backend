import { arbitrumIngressEgressFailedDepositQueuedEvent } from '../../arbitrumIngressEgress/failedDepositQueued';
import { assethubIngressEgressFailedDepositQueuedEvent } from '../../assethubIngressEgress/failedDepositQueued';
import { bitcoinIngressEgressFailedDepositQueuedEvent } from '../../bitcoinIngressEgress/failedDepositQueued';
import { bscIngressEgressFailedDepositQueuedEvent } from '../../bscIngressEgress/failedDepositQueued';
import { ethereumIngressEgressFailedDepositQueuedEvent } from '../../ethereumIngressEgress/failedDepositQueued';
import { polkadotIngressEgressFailedDepositQueuedEvent } from '../../polkadotIngressEgress/failedDepositQueued';
import { solanaIngressEgressFailedDepositQueuedEvent } from '../../solanaIngressEgress/failedDepositQueued';
import { tronIngressEgressFailedDepositQueuedEvent } from '../../tronIngressEgress/failedDepositQueued';

export const ingressEgressFailedDepositQueuedEvent = {
  Arbitrum: arbitrumIngressEgressFailedDepositQueuedEvent,
  Assethub: assethubIngressEgressFailedDepositQueuedEvent,
  Bitcoin: bitcoinIngressEgressFailedDepositQueuedEvent,
  Bsc: bscIngressEgressFailedDepositQueuedEvent,
  Ethereum: ethereumIngressEgressFailedDepositQueuedEvent,
  Polkadot: polkadotIngressEgressFailedDepositQueuedEvent,
  Solana: solanaIngressEgressFailedDepositQueuedEvent,
  Tron: tronIngressEgressFailedDepositQueuedEvent,
} as const;
