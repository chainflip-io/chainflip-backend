import { arbitrumIngressEgressInvalidCcmRefundedEvent } from '../../arbitrumIngressEgress/invalidCcmRefunded';
import { assethubIngressEgressInvalidCcmRefundedEvent } from '../../assethubIngressEgress/invalidCcmRefunded';
import { bitcoinIngressEgressInvalidCcmRefundedEvent } from '../../bitcoinIngressEgress/invalidCcmRefunded';
import { bscIngressEgressInvalidCcmRefundedEvent } from '../../bscIngressEgress/invalidCcmRefunded';
import { ethereumIngressEgressInvalidCcmRefundedEvent } from '../../ethereumIngressEgress/invalidCcmRefunded';
import { polkadotIngressEgressInvalidCcmRefundedEvent } from '../../polkadotIngressEgress/invalidCcmRefunded';
import { solanaIngressEgressInvalidCcmRefundedEvent } from '../../solanaIngressEgress/invalidCcmRefunded';
import { tronIngressEgressInvalidCcmRefundedEvent } from '../../tronIngressEgress/invalidCcmRefunded';

export const ingressEgressInvalidCcmRefundedEvent = {
  Arbitrum: arbitrumIngressEgressInvalidCcmRefundedEvent,
  Assethub: assethubIngressEgressInvalidCcmRefundedEvent,
  Bitcoin: bitcoinIngressEgressInvalidCcmRefundedEvent,
  Bsc: bscIngressEgressInvalidCcmRefundedEvent,
  Ethereum: ethereumIngressEgressInvalidCcmRefundedEvent,
  Polkadot: polkadotIngressEgressInvalidCcmRefundedEvent,
  Solana: solanaIngressEgressInvalidCcmRefundedEvent,
  Tron: tronIngressEgressInvalidCcmRefundedEvent,
} as const;
