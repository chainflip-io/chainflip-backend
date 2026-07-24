import { arbitrumBroadcasterThresholdSignatureInvalidEvent } from '../../arbitrumBroadcaster/thresholdSignatureInvalid';
import { assethubBroadcasterThresholdSignatureInvalidEvent } from '../../assethubBroadcaster/thresholdSignatureInvalid';
import { bitcoinBroadcasterThresholdSignatureInvalidEvent } from '../../bitcoinBroadcaster/thresholdSignatureInvalid';
import { bscBroadcasterThresholdSignatureInvalidEvent } from '../../bscBroadcaster/thresholdSignatureInvalid';
import { ethereumBroadcasterThresholdSignatureInvalidEvent } from '../../ethereumBroadcaster/thresholdSignatureInvalid';
import { polkadotBroadcasterThresholdSignatureInvalidEvent } from '../../polkadotBroadcaster/thresholdSignatureInvalid';
import { solanaBroadcasterThresholdSignatureInvalidEvent } from '../../solanaBroadcaster/thresholdSignatureInvalid';
import { tronBroadcasterThresholdSignatureInvalidEvent } from '../../tronBroadcaster/thresholdSignatureInvalid';

export const broadcasterThresholdSignatureInvalidEvent = {
  Arbitrum: arbitrumBroadcasterThresholdSignatureInvalidEvent,
  Assethub: assethubBroadcasterThresholdSignatureInvalidEvent,
  Bitcoin: bitcoinBroadcasterThresholdSignatureInvalidEvent,
  Bsc: bscBroadcasterThresholdSignatureInvalidEvent,
  Ethereum: ethereumBroadcasterThresholdSignatureInvalidEvent,
  Polkadot: polkadotBroadcasterThresholdSignatureInvalidEvent,
  Solana: solanaBroadcasterThresholdSignatureInvalidEvent,
  Tron: tronBroadcasterThresholdSignatureInvalidEvent,
} as const;
