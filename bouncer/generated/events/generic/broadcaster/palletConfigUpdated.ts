import { arbitrumBroadcasterPalletConfigUpdatedEvent } from '../../arbitrumBroadcaster/palletConfigUpdated';
import { assethubBroadcasterPalletConfigUpdatedEvent } from '../../assethubBroadcaster/palletConfigUpdated';
import { bitcoinBroadcasterPalletConfigUpdatedEvent } from '../../bitcoinBroadcaster/palletConfigUpdated';
import { bscBroadcasterPalletConfigUpdatedEvent } from '../../bscBroadcaster/palletConfigUpdated';
import { ethereumBroadcasterPalletConfigUpdatedEvent } from '../../ethereumBroadcaster/palletConfigUpdated';
import { polkadotBroadcasterPalletConfigUpdatedEvent } from '../../polkadotBroadcaster/palletConfigUpdated';
import { solanaBroadcasterPalletConfigUpdatedEvent } from '../../solanaBroadcaster/palletConfigUpdated';
import { tronBroadcasterPalletConfigUpdatedEvent } from '../../tronBroadcaster/palletConfigUpdated';

export const broadcasterPalletConfigUpdatedEvent = {
  Arbitrum: arbitrumBroadcasterPalletConfigUpdatedEvent,
  Assethub: assethubBroadcasterPalletConfigUpdatedEvent,
  Bitcoin: bitcoinBroadcasterPalletConfigUpdatedEvent,
  Bsc: bscBroadcasterPalletConfigUpdatedEvent,
  Ethereum: ethereumBroadcasterPalletConfigUpdatedEvent,
  Polkadot: polkadotBroadcasterPalletConfigUpdatedEvent,
  Solana: solanaBroadcasterPalletConfigUpdatedEvent,
  Tron: tronBroadcasterPalletConfigUpdatedEvent,
} as const;
