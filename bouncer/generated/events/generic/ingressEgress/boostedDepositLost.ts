import { arbitrumIngressEgressBoostedDepositLostEvent } from '../../arbitrumIngressEgress/boostedDepositLost';
import { assethubIngressEgressBoostedDepositLostEvent } from '../../assethubIngressEgress/boostedDepositLost';
import { bitcoinIngressEgressBoostedDepositLostEvent } from '../../bitcoinIngressEgress/boostedDepositLost';
import { bscIngressEgressBoostedDepositLostEvent } from '../../bscIngressEgress/boostedDepositLost';
import { ethereumIngressEgressBoostedDepositLostEvent } from '../../ethereumIngressEgress/boostedDepositLost';
import { polkadotIngressEgressBoostedDepositLostEvent } from '../../polkadotIngressEgress/boostedDepositLost';
import { solanaIngressEgressBoostedDepositLostEvent } from '../../solanaIngressEgress/boostedDepositLost';
import { tronIngressEgressBoostedDepositLostEvent } from '../../tronIngressEgress/boostedDepositLost';

export const ingressEgressBoostedDepositLostEvent = {
  Arbitrum: arbitrumIngressEgressBoostedDepositLostEvent,
  Assethub: assethubIngressEgressBoostedDepositLostEvent,
  Bitcoin: bitcoinIngressEgressBoostedDepositLostEvent,
  Bsc: bscIngressEgressBoostedDepositLostEvent,
  Ethereum: ethereumIngressEgressBoostedDepositLostEvent,
  Polkadot: polkadotIngressEgressBoostedDepositLostEvent,
  Solana: solanaIngressEgressBoostedDepositLostEvent,
  Tron: tronIngressEgressBoostedDepositLostEvent,
} as const;
