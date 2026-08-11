import { arbitrumIngressEgressCcmEgressInvalidEvent } from '../../arbitrumIngressEgress/ccmEgressInvalid';
import { assethubIngressEgressCcmEgressInvalidEvent } from '../../assethubIngressEgress/ccmEgressInvalid';
import { bitcoinIngressEgressCcmEgressInvalidEvent } from '../../bitcoinIngressEgress/ccmEgressInvalid';
import { bscIngressEgressCcmEgressInvalidEvent } from '../../bscIngressEgress/ccmEgressInvalid';
import { ethereumIngressEgressCcmEgressInvalidEvent } from '../../ethereumIngressEgress/ccmEgressInvalid';
import { polkadotIngressEgressCcmEgressInvalidEvent } from '../../polkadotIngressEgress/ccmEgressInvalid';
import { solanaIngressEgressCcmEgressInvalidEvent } from '../../solanaIngressEgress/ccmEgressInvalid';
import { tronIngressEgressCcmEgressInvalidEvent } from '../../tronIngressEgress/ccmEgressInvalid';

export const ingressEgressCcmEgressInvalidEvent = {
  Arbitrum: arbitrumIngressEgressCcmEgressInvalidEvent,
  Assethub: assethubIngressEgressCcmEgressInvalidEvent,
  Bitcoin: bitcoinIngressEgressCcmEgressInvalidEvent,
  Bsc: bscIngressEgressCcmEgressInvalidEvent,
  Ethereum: ethereumIngressEgressCcmEgressInvalidEvent,
  Polkadot: polkadotIngressEgressCcmEgressInvalidEvent,
  Solana: solanaIngressEgressCcmEgressInvalidEvent,
  Tron: tronIngressEgressCcmEgressInvalidEvent,
} as const;
