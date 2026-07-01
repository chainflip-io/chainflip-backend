import { arbitrumIngressEgressTransactionRejectionRequestReceivedEvent } from '../../arbitrumIngressEgress/transactionRejectionRequestReceived';
import { assethubIngressEgressTransactionRejectionRequestReceivedEvent } from '../../assethubIngressEgress/transactionRejectionRequestReceived';
import { bitcoinIngressEgressTransactionRejectionRequestReceivedEvent } from '../../bitcoinIngressEgress/transactionRejectionRequestReceived';
import { bscIngressEgressTransactionRejectionRequestReceivedEvent } from '../../bscIngressEgress/transactionRejectionRequestReceived';
import { ethereumIngressEgressTransactionRejectionRequestReceivedEvent } from '../../ethereumIngressEgress/transactionRejectionRequestReceived';
import { polkadotIngressEgressTransactionRejectionRequestReceivedEvent } from '../../polkadotIngressEgress/transactionRejectionRequestReceived';
import { solanaIngressEgressTransactionRejectionRequestReceivedEvent } from '../../solanaIngressEgress/transactionRejectionRequestReceived';
import { tronIngressEgressTransactionRejectionRequestReceivedEvent } from '../../tronIngressEgress/transactionRejectionRequestReceived';

export const ingressEgressTransactionRejectionRequestReceivedEvent = {
  Arbitrum: arbitrumIngressEgressTransactionRejectionRequestReceivedEvent,
  Assethub: assethubIngressEgressTransactionRejectionRequestReceivedEvent,
  Bitcoin: bitcoinIngressEgressTransactionRejectionRequestReceivedEvent,
  Bsc: bscIngressEgressTransactionRejectionRequestReceivedEvent,
  Ethereum: ethereumIngressEgressTransactionRejectionRequestReceivedEvent,
  Polkadot: polkadotIngressEgressTransactionRejectionRequestReceivedEvent,
  Solana: solanaIngressEgressTransactionRejectionRequestReceivedEvent,
  Tron: tronIngressEgressTransactionRejectionRequestReceivedEvent,
} as const;
