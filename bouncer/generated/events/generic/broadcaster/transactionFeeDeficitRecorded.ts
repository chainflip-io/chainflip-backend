import { arbitrumBroadcasterTransactionFeeDeficitRecordedEvent } from '../../arbitrumBroadcaster/transactionFeeDeficitRecorded';
import { assethubBroadcasterTransactionFeeDeficitRecordedEvent } from '../../assethubBroadcaster/transactionFeeDeficitRecorded';
import { bitcoinBroadcasterTransactionFeeDeficitRecordedEvent } from '../../bitcoinBroadcaster/transactionFeeDeficitRecorded';
import { bscBroadcasterTransactionFeeDeficitRecordedEvent } from '../../bscBroadcaster/transactionFeeDeficitRecorded';
import { ethereumBroadcasterTransactionFeeDeficitRecordedEvent } from '../../ethereumBroadcaster/transactionFeeDeficitRecorded';
import { polkadotBroadcasterTransactionFeeDeficitRecordedEvent } from '../../polkadotBroadcaster/transactionFeeDeficitRecorded';
import { solanaBroadcasterTransactionFeeDeficitRecordedEvent } from '../../solanaBroadcaster/transactionFeeDeficitRecorded';
import { tronBroadcasterTransactionFeeDeficitRecordedEvent } from '../../tronBroadcaster/transactionFeeDeficitRecorded';

export const broadcasterTransactionFeeDeficitRecordedEvent = {
  Arbitrum: arbitrumBroadcasterTransactionFeeDeficitRecordedEvent,
  Assethub: assethubBroadcasterTransactionFeeDeficitRecordedEvent,
  Bitcoin: bitcoinBroadcasterTransactionFeeDeficitRecordedEvent,
  Bsc: bscBroadcasterTransactionFeeDeficitRecordedEvent,
  Ethereum: ethereumBroadcasterTransactionFeeDeficitRecordedEvent,
  Polkadot: polkadotBroadcasterTransactionFeeDeficitRecordedEvent,
  Solana: solanaBroadcasterTransactionFeeDeficitRecordedEvent,
  Tron: tronBroadcasterTransactionFeeDeficitRecordedEvent,
} as const;
