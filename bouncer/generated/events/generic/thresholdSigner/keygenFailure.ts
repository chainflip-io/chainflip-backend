import { bitcoinThresholdSignerKeygenFailureEvent } from '../../bitcoinThresholdSigner/keygenFailure';
import { evmThresholdSignerKeygenFailureEvent } from '../../evmThresholdSigner/keygenFailure';
import { polkadotThresholdSignerKeygenFailureEvent } from '../../polkadotThresholdSigner/keygenFailure';
import { solanaThresholdSignerKeygenFailureEvent } from '../../solanaThresholdSigner/keygenFailure';

export const thresholdSignerKeygenFailureEvent = {
  Bitcoin: bitcoinThresholdSignerKeygenFailureEvent,
  Evm: evmThresholdSignerKeygenFailureEvent,
  Polkadot: polkadotThresholdSignerKeygenFailureEvent,
  Solana: solanaThresholdSignerKeygenFailureEvent,
} as const;
