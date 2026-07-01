import { evmThresholdSignerKeygenFailureEvent } from '../../evmThresholdSigner/keygenFailure';
import { polkadotThresholdSignerKeygenFailureEvent } from '../../polkadotThresholdSigner/keygenFailure';
import { bitcoinThresholdSignerKeygenFailureEvent } from '../../bitcoinThresholdSigner/keygenFailure';
import { solanaThresholdSignerKeygenFailureEvent } from '../../solanaThresholdSigner/keygenFailure';

export const thresholdSignerKeygenFailureEvent = {
  Arbitrum: evmThresholdSignerKeygenFailureEvent,
  Assethub: polkadotThresholdSignerKeygenFailureEvent,
  Bitcoin: bitcoinThresholdSignerKeygenFailureEvent,
  Ethereum: evmThresholdSignerKeygenFailureEvent,
  Polkadot: polkadotThresholdSignerKeygenFailureEvent,
  Solana: solanaThresholdSignerKeygenFailureEvent,
} as const;
