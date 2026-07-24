import { arbitrumBroadcasterTransactionFeeDeficitRefusedEvent } from '../../arbitrumBroadcaster/transactionFeeDeficitRefused';
import { assethubBroadcasterTransactionFeeDeficitRefusedEvent } from '../../assethubBroadcaster/transactionFeeDeficitRefused';
import { bitcoinBroadcasterTransactionFeeDeficitRefusedEvent } from '../../bitcoinBroadcaster/transactionFeeDeficitRefused';
import { bscBroadcasterTransactionFeeDeficitRefusedEvent } from '../../bscBroadcaster/transactionFeeDeficitRefused';
import { ethereumBroadcasterTransactionFeeDeficitRefusedEvent } from '../../ethereumBroadcaster/transactionFeeDeficitRefused';
import { polkadotBroadcasterTransactionFeeDeficitRefusedEvent } from '../../polkadotBroadcaster/transactionFeeDeficitRefused';
import { solanaBroadcasterTransactionFeeDeficitRefusedEvent } from '../../solanaBroadcaster/transactionFeeDeficitRefused';
import { tronBroadcasterTransactionFeeDeficitRefusedEvent } from '../../tronBroadcaster/transactionFeeDeficitRefused';

export const broadcasterTransactionFeeDeficitRefusedEvent = {
  Arbitrum: arbitrumBroadcasterTransactionFeeDeficitRefusedEvent,
  Assethub: assethubBroadcasterTransactionFeeDeficitRefusedEvent,
  Bitcoin: bitcoinBroadcasterTransactionFeeDeficitRefusedEvent,
  Bsc: bscBroadcasterTransactionFeeDeficitRefusedEvent,
  Ethereum: ethereumBroadcasterTransactionFeeDeficitRefusedEvent,
  Polkadot: polkadotBroadcasterTransactionFeeDeficitRefusedEvent,
  Solana: solanaBroadcasterTransactionFeeDeficitRefusedEvent,
  Tron: tronBroadcasterTransactionFeeDeficitRefusedEvent,
} as const;
