import { arbitrumBroadcasterBroadcastTimeoutEvent } from '../../arbitrumBroadcaster/broadcastTimeout';
import { assethubBroadcasterBroadcastTimeoutEvent } from '../../assethubBroadcaster/broadcastTimeout';
import { bitcoinBroadcasterBroadcastTimeoutEvent } from '../../bitcoinBroadcaster/broadcastTimeout';
import { bscBroadcasterBroadcastTimeoutEvent } from '../../bscBroadcaster/broadcastTimeout';
import { ethereumBroadcasterBroadcastTimeoutEvent } from '../../ethereumBroadcaster/broadcastTimeout';
import { polkadotBroadcasterBroadcastTimeoutEvent } from '../../polkadotBroadcaster/broadcastTimeout';
import { solanaBroadcasterBroadcastTimeoutEvent } from '../../solanaBroadcaster/broadcastTimeout';
import { tronBroadcasterBroadcastTimeoutEvent } from '../../tronBroadcaster/broadcastTimeout';

export const broadcasterBroadcastTimeoutEvent = {
  Arbitrum: arbitrumBroadcasterBroadcastTimeoutEvent,
  Assethub: assethubBroadcasterBroadcastTimeoutEvent,
  Bitcoin: bitcoinBroadcasterBroadcastTimeoutEvent,
  Bsc: bscBroadcasterBroadcastTimeoutEvent,
  Ethereum: ethereumBroadcasterBroadcastTimeoutEvent,
  Polkadot: polkadotBroadcasterBroadcastTimeoutEvent,
  Solana: solanaBroadcasterBroadcastTimeoutEvent,
  Tron: tronBroadcasterBroadcastTimeoutEvent,
} as const;
