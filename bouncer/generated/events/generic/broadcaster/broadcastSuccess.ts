import { arbitrumBroadcasterBroadcastSuccessEvent } from '../../arbitrumBroadcaster/broadcastSuccess';
import { assethubBroadcasterBroadcastSuccessEvent } from '../../assethubBroadcaster/broadcastSuccess';
import { bitcoinBroadcasterBroadcastSuccessEvent } from '../../bitcoinBroadcaster/broadcastSuccess';
import { bscBroadcasterBroadcastSuccessEvent } from '../../bscBroadcaster/broadcastSuccess';
import { ethereumBroadcasterBroadcastSuccessEvent } from '../../ethereumBroadcaster/broadcastSuccess';
import { polkadotBroadcasterBroadcastSuccessEvent } from '../../polkadotBroadcaster/broadcastSuccess';
import { solanaBroadcasterBroadcastSuccessEvent } from '../../solanaBroadcaster/broadcastSuccess';
import { tronBroadcasterBroadcastSuccessEvent } from '../../tronBroadcaster/broadcastSuccess';

export const broadcasterBroadcastSuccessEvent = {
  Arbitrum: arbitrumBroadcasterBroadcastSuccessEvent,
  Assethub: assethubBroadcasterBroadcastSuccessEvent,
  Bitcoin: bitcoinBroadcasterBroadcastSuccessEvent,
  Bsc: bscBroadcasterBroadcastSuccessEvent,
  Ethereum: ethereumBroadcasterBroadcastSuccessEvent,
  Polkadot: polkadotBroadcasterBroadcastSuccessEvent,
  Solana: solanaBroadcasterBroadcastSuccessEvent,
  Tron: tronBroadcasterBroadcastSuccessEvent,
} as const;
