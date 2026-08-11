import { arbitrumIngressEgressTransferFallbackRequestedEvent } from '../../arbitrumIngressEgress/transferFallbackRequested';
import { assethubIngressEgressTransferFallbackRequestedEvent } from '../../assethubIngressEgress/transferFallbackRequested';
import { bitcoinIngressEgressTransferFallbackRequestedEvent } from '../../bitcoinIngressEgress/transferFallbackRequested';
import { bscIngressEgressTransferFallbackRequestedEvent } from '../../bscIngressEgress/transferFallbackRequested';
import { ethereumIngressEgressTransferFallbackRequestedEvent } from '../../ethereumIngressEgress/transferFallbackRequested';
import { polkadotIngressEgressTransferFallbackRequestedEvent } from '../../polkadotIngressEgress/transferFallbackRequested';
import { solanaIngressEgressTransferFallbackRequestedEvent } from '../../solanaIngressEgress/transferFallbackRequested';
import { tronIngressEgressTransferFallbackRequestedEvent } from '../../tronIngressEgress/transferFallbackRequested';

export const ingressEgressTransferFallbackRequestedEvent = {
  Arbitrum: arbitrumIngressEgressTransferFallbackRequestedEvent,
  Assethub: assethubIngressEgressTransferFallbackRequestedEvent,
  Bitcoin: bitcoinIngressEgressTransferFallbackRequestedEvent,
  Bsc: bscIngressEgressTransferFallbackRequestedEvent,
  Ethereum: ethereumIngressEgressTransferFallbackRequestedEvent,
  Polkadot: polkadotIngressEgressTransferFallbackRequestedEvent,
  Solana: solanaIngressEgressTransferFallbackRequestedEvent,
  Tron: tronIngressEgressTransferFallbackRequestedEvent,
} as const;
