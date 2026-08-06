import { arbitrumIngressEgressCcmBroadcastFailedEvent } from '../../arbitrumIngressEgress/ccmBroadcastFailed';
import { assethubIngressEgressCcmBroadcastFailedEvent } from '../../assethubIngressEgress/ccmBroadcastFailed';
import { bitcoinIngressEgressCcmBroadcastFailedEvent } from '../../bitcoinIngressEgress/ccmBroadcastFailed';
import { bscIngressEgressCcmBroadcastFailedEvent } from '../../bscIngressEgress/ccmBroadcastFailed';
import { ethereumIngressEgressCcmBroadcastFailedEvent } from '../../ethereumIngressEgress/ccmBroadcastFailed';
import { polkadotIngressEgressCcmBroadcastFailedEvent } from '../../polkadotIngressEgress/ccmBroadcastFailed';
import { solanaIngressEgressCcmBroadcastFailedEvent } from '../../solanaIngressEgress/ccmBroadcastFailed';
import { tronIngressEgressCcmBroadcastFailedEvent } from '../../tronIngressEgress/ccmBroadcastFailed';

export const ingressEgressCcmBroadcastFailedEvent = {
  Arbitrum: arbitrumIngressEgressCcmBroadcastFailedEvent,
  Assethub: assethubIngressEgressCcmBroadcastFailedEvent,
  Bitcoin: bitcoinIngressEgressCcmBroadcastFailedEvent,
  Bsc: bscIngressEgressCcmBroadcastFailedEvent,
  Ethereum: ethereumIngressEgressCcmBroadcastFailedEvent,
  Polkadot: polkadotIngressEgressCcmBroadcastFailedEvent,
  Solana: solanaIngressEgressCcmBroadcastFailedEvent,
  Tron: tronIngressEgressCcmBroadcastFailedEvent,
} as const;
