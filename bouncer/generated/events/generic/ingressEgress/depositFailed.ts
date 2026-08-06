import { arbitrumIngressEgressDepositFailedEvent } from '../../arbitrumIngressEgress/depositFailed';
import { assethubIngressEgressDepositFailedEvent } from '../../assethubIngressEgress/depositFailed';
import { bitcoinIngressEgressDepositFailedEvent } from '../../bitcoinIngressEgress/depositFailed';
import { bscIngressEgressDepositFailedEvent } from '../../bscIngressEgress/depositFailed';
import { ethereumIngressEgressDepositFailedEvent } from '../../ethereumIngressEgress/depositFailed';
import { polkadotIngressEgressDepositFailedEvent } from '../../polkadotIngressEgress/depositFailed';
import { solanaIngressEgressDepositFailedEvent } from '../../solanaIngressEgress/depositFailed';
import { tronIngressEgressDepositFailedEvent } from '../../tronIngressEgress/depositFailed';

export const ingressEgressDepositFailedEvent = {
  Arbitrum: arbitrumIngressEgressDepositFailedEvent,
  Assethub: assethubIngressEgressDepositFailedEvent,
  Bitcoin: bitcoinIngressEgressDepositFailedEvent,
  Bsc: bscIngressEgressDepositFailedEvent,
  Ethereum: ethereumIngressEgressDepositFailedEvent,
  Polkadot: polkadotIngressEgressDepositFailedEvent,
  Solana: solanaIngressEgressDepositFailedEvent,
  Tron: tronIngressEgressDepositFailedEvent,
} as const;
