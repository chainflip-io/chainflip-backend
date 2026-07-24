import { arbitrumIngressEgressBatchBroadcastRequestedEvent } from '../../arbitrumIngressEgress/batchBroadcastRequested';
import { assethubIngressEgressBatchBroadcastRequestedEvent } from '../../assethubIngressEgress/batchBroadcastRequested';
import { bitcoinIngressEgressBatchBroadcastRequestedEvent } from '../../bitcoinIngressEgress/batchBroadcastRequested';
import { bscIngressEgressBatchBroadcastRequestedEvent } from '../../bscIngressEgress/batchBroadcastRequested';
import { ethereumIngressEgressBatchBroadcastRequestedEvent } from '../../ethereumIngressEgress/batchBroadcastRequested';
import { polkadotIngressEgressBatchBroadcastRequestedEvent } from '../../polkadotIngressEgress/batchBroadcastRequested';
import { solanaIngressEgressBatchBroadcastRequestedEvent } from '../../solanaIngressEgress/batchBroadcastRequested';
import { tronIngressEgressBatchBroadcastRequestedEvent } from '../../tronIngressEgress/batchBroadcastRequested';

export const ingressEgressBatchBroadcastRequestedEvent = {
  Arbitrum: arbitrumIngressEgressBatchBroadcastRequestedEvent,
  Assethub: assethubIngressEgressBatchBroadcastRequestedEvent,
  Bitcoin: bitcoinIngressEgressBatchBroadcastRequestedEvent,
  Bsc: bscIngressEgressBatchBroadcastRequestedEvent,
  Ethereum: ethereumIngressEgressBatchBroadcastRequestedEvent,
  Polkadot: polkadotIngressEgressBatchBroadcastRequestedEvent,
  Solana: solanaIngressEgressBatchBroadcastRequestedEvent,
  Tron: tronIngressEgressBatchBroadcastRequestedEvent,
} as const;
