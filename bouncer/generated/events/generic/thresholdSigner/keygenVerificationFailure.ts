import { bitcoinThresholdSignerKeygenVerificationFailureEvent } from '../../bitcoinThresholdSigner/keygenVerificationFailure';
import { evmThresholdSignerKeygenVerificationFailureEvent } from '../../evmThresholdSigner/keygenVerificationFailure';
import { polkadotThresholdSignerKeygenVerificationFailureEvent } from '../../polkadotThresholdSigner/keygenVerificationFailure';
import { solanaThresholdSignerKeygenVerificationFailureEvent } from '../../solanaThresholdSigner/keygenVerificationFailure';

export const thresholdSignerKeygenVerificationFailureEvent = {
  Bitcoin: bitcoinThresholdSignerKeygenVerificationFailureEvent,
  Evm: evmThresholdSignerKeygenVerificationFailureEvent,
  Polkadot: polkadotThresholdSignerKeygenVerificationFailureEvent,
  Solana: solanaThresholdSignerKeygenVerificationFailureEvent,
} as const;
