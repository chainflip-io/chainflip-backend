import { arbitrumIngressEgressTransactionRejectedByBrokerEvent } from '../../arbitrumIngressEgress/transactionRejectedByBroker';
import { assethubIngressEgressTransactionRejectedByBrokerEvent } from '../../assethubIngressEgress/transactionRejectedByBroker';
import { bitcoinIngressEgressTransactionRejectedByBrokerEvent } from '../../bitcoinIngressEgress/transactionRejectedByBroker';
import { bscIngressEgressTransactionRejectedByBrokerEvent } from '../../bscIngressEgress/transactionRejectedByBroker';
import { ethereumIngressEgressTransactionRejectedByBrokerEvent } from '../../ethereumIngressEgress/transactionRejectedByBroker';
import { polkadotIngressEgressTransactionRejectedByBrokerEvent } from '../../polkadotIngressEgress/transactionRejectedByBroker';
import { solanaIngressEgressTransactionRejectedByBrokerEvent } from '../../solanaIngressEgress/transactionRejectedByBroker';
import { tronIngressEgressTransactionRejectedByBrokerEvent } from '../../tronIngressEgress/transactionRejectedByBroker';

export const ingressEgressTransactionRejectedByBrokerEvent = {
  Arbitrum: arbitrumIngressEgressTransactionRejectedByBrokerEvent,
  Assethub: assethubIngressEgressTransactionRejectedByBrokerEvent,
  Bitcoin: bitcoinIngressEgressTransactionRejectedByBrokerEvent,
  Bsc: bscIngressEgressTransactionRejectedByBrokerEvent,
  Ethereum: ethereumIngressEgressTransactionRejectedByBrokerEvent,
  Polkadot: polkadotIngressEgressTransactionRejectedByBrokerEvent,
  Solana: solanaIngressEgressTransactionRejectedByBrokerEvent,
  Tron: tronIngressEgressTransactionRejectedByBrokerEvent,
} as const;
