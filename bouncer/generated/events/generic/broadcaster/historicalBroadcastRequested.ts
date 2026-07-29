import { arbitrumBroadcasterHistoricalBroadcastRequestedEvent } from '../../arbitrumBroadcaster/historicalBroadcastRequested';
import { assethubBroadcasterHistoricalBroadcastRequestedEvent } from '../../assethubBroadcaster/historicalBroadcastRequested';
import { bitcoinBroadcasterHistoricalBroadcastRequestedEvent } from '../../bitcoinBroadcaster/historicalBroadcastRequested';
import { bscBroadcasterHistoricalBroadcastRequestedEvent } from '../../bscBroadcaster/historicalBroadcastRequested';
import { ethereumBroadcasterHistoricalBroadcastRequestedEvent } from '../../ethereumBroadcaster/historicalBroadcastRequested';
import { polkadotBroadcasterHistoricalBroadcastRequestedEvent } from '../../polkadotBroadcaster/historicalBroadcastRequested';
import { solanaBroadcasterHistoricalBroadcastRequestedEvent } from '../../solanaBroadcaster/historicalBroadcastRequested';
import { tronBroadcasterHistoricalBroadcastRequestedEvent } from '../../tronBroadcaster/historicalBroadcastRequested';

export const broadcasterHistoricalBroadcastRequestedEvent = {
  Arbitrum: arbitrumBroadcasterHistoricalBroadcastRequestedEvent,
  Assethub: assethubBroadcasterHistoricalBroadcastRequestedEvent,
  Bitcoin: bitcoinBroadcasterHistoricalBroadcastRequestedEvent,
  Bsc: bscBroadcasterHistoricalBroadcastRequestedEvent,
  Ethereum: ethereumBroadcasterHistoricalBroadcastRequestedEvent,
  Polkadot: polkadotBroadcasterHistoricalBroadcastRequestedEvent,
  Solana: solanaBroadcasterHistoricalBroadcastRequestedEvent,
  Tron: tronBroadcasterHistoricalBroadcastRequestedEvent,
} as const;
