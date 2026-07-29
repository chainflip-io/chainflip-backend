import { arbitrumBroadcasterBroadcastRetryScheduledEvent } from '../../arbitrumBroadcaster/broadcastRetryScheduled';
import { assethubBroadcasterBroadcastRetryScheduledEvent } from '../../assethubBroadcaster/broadcastRetryScheduled';
import { bitcoinBroadcasterBroadcastRetryScheduledEvent } from '../../bitcoinBroadcaster/broadcastRetryScheduled';
import { bscBroadcasterBroadcastRetryScheduledEvent } from '../../bscBroadcaster/broadcastRetryScheduled';
import { ethereumBroadcasterBroadcastRetryScheduledEvent } from '../../ethereumBroadcaster/broadcastRetryScheduled';
import { polkadotBroadcasterBroadcastRetryScheduledEvent } from '../../polkadotBroadcaster/broadcastRetryScheduled';
import { solanaBroadcasterBroadcastRetryScheduledEvent } from '../../solanaBroadcaster/broadcastRetryScheduled';
import { tronBroadcasterBroadcastRetryScheduledEvent } from '../../tronBroadcaster/broadcastRetryScheduled';

export const broadcasterBroadcastRetryScheduledEvent = {
  Arbitrum: arbitrumBroadcasterBroadcastRetryScheduledEvent,
  Assethub: assethubBroadcasterBroadcastRetryScheduledEvent,
  Bitcoin: bitcoinBroadcasterBroadcastRetryScheduledEvent,
  Bsc: bscBroadcasterBroadcastRetryScheduledEvent,
  Ethereum: ethereumBroadcasterBroadcastRetryScheduledEvent,
  Polkadot: polkadotBroadcasterBroadcastRetryScheduledEvent,
  Solana: solanaBroadcasterBroadcastRetryScheduledEvent,
  Tron: tronBroadcasterBroadcastRetryScheduledEvent,
} as const;
