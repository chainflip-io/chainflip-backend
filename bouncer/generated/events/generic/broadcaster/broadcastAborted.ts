import { arbitrumBroadcasterBroadcastAbortedEvent } from '../../arbitrumBroadcaster/broadcastAborted';
import { assethubBroadcasterBroadcastAbortedEvent } from '../../assethubBroadcaster/broadcastAborted';
import { bitcoinBroadcasterBroadcastAbortedEvent } from '../../bitcoinBroadcaster/broadcastAborted';
import { bscBroadcasterBroadcastAbortedEvent } from '../../bscBroadcaster/broadcastAborted';
import { ethereumBroadcasterBroadcastAbortedEvent } from '../../ethereumBroadcaster/broadcastAborted';
import { polkadotBroadcasterBroadcastAbortedEvent } from '../../polkadotBroadcaster/broadcastAborted';
import { solanaBroadcasterBroadcastAbortedEvent } from '../../solanaBroadcaster/broadcastAborted';
import { tronBroadcasterBroadcastAbortedEvent } from '../../tronBroadcaster/broadcastAborted';

export const broadcasterBroadcastAbortedEvent = {
  Arbitrum: arbitrumBroadcasterBroadcastAbortedEvent,
  Assethub: assethubBroadcasterBroadcastAbortedEvent,
  Bitcoin: bitcoinBroadcasterBroadcastAbortedEvent,
  Bsc: bscBroadcasterBroadcastAbortedEvent,
  Ethereum: ethereumBroadcasterBroadcastAbortedEvent,
  Polkadot: polkadotBroadcasterBroadcastAbortedEvent,
  Solana: solanaBroadcasterBroadcastAbortedEvent,
  Tron: tronBroadcasterBroadcastAbortedEvent,
} as const;
