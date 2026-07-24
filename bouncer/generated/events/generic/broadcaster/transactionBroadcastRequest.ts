import { arbitrumBroadcasterTransactionBroadcastRequestEvent } from '../../arbitrumBroadcaster/transactionBroadcastRequest';
import { assethubBroadcasterTransactionBroadcastRequestEvent } from '../../assethubBroadcaster/transactionBroadcastRequest';
import { bitcoinBroadcasterTransactionBroadcastRequestEvent } from '../../bitcoinBroadcaster/transactionBroadcastRequest';
import { bscBroadcasterTransactionBroadcastRequestEvent } from '../../bscBroadcaster/transactionBroadcastRequest';
import { ethereumBroadcasterTransactionBroadcastRequestEvent } from '../../ethereumBroadcaster/transactionBroadcastRequest';
import { polkadotBroadcasterTransactionBroadcastRequestEvent } from '../../polkadotBroadcaster/transactionBroadcastRequest';
import { solanaBroadcasterTransactionBroadcastRequestEvent } from '../../solanaBroadcaster/transactionBroadcastRequest';
import { tronBroadcasterTransactionBroadcastRequestEvent } from '../../tronBroadcaster/transactionBroadcastRequest';

export const broadcasterTransactionBroadcastRequestEvent = {
  Arbitrum: arbitrumBroadcasterTransactionBroadcastRequestEvent,
  Assethub: assethubBroadcasterTransactionBroadcastRequestEvent,
  Bitcoin: bitcoinBroadcasterTransactionBroadcastRequestEvent,
  Bsc: bscBroadcasterTransactionBroadcastRequestEvent,
  Ethereum: ethereumBroadcasterTransactionBroadcastRequestEvent,
  Polkadot: polkadotBroadcasterTransactionBroadcastRequestEvent,
  Solana: solanaBroadcasterTransactionBroadcastRequestEvent,
  Tron: tronBroadcasterTransactionBroadcastRequestEvent,
} as const;
