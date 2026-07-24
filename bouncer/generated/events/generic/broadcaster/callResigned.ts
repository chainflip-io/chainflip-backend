import { arbitrumBroadcasterCallResignedEvent } from '../../arbitrumBroadcaster/callResigned';
import { assethubBroadcasterCallResignedEvent } from '../../assethubBroadcaster/callResigned';
import { bitcoinBroadcasterCallResignedEvent } from '../../bitcoinBroadcaster/callResigned';
import { bscBroadcasterCallResignedEvent } from '../../bscBroadcaster/callResigned';
import { ethereumBroadcasterCallResignedEvent } from '../../ethereumBroadcaster/callResigned';
import { polkadotBroadcasterCallResignedEvent } from '../../polkadotBroadcaster/callResigned';
import { solanaBroadcasterCallResignedEvent } from '../../solanaBroadcaster/callResigned';
import { tronBroadcasterCallResignedEvent } from '../../tronBroadcaster/callResigned';

export const broadcasterCallResignedEvent = {
  Arbitrum: arbitrumBroadcasterCallResignedEvent,
  Assethub: assethubBroadcasterCallResignedEvent,
  Bitcoin: bitcoinBroadcasterCallResignedEvent,
  Bsc: bscBroadcasterCallResignedEvent,
  Ethereum: ethereumBroadcasterCallResignedEvent,
  Polkadot: polkadotBroadcasterCallResignedEvent,
  Solana: solanaBroadcasterCallResignedEvent,
  Tron: tronBroadcasterCallResignedEvent,
} as const;
