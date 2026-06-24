import { evmThresholdSignerKeygenVerificationFailureEvent } from '../../evmThresholdSigner/keygenVerificationFailure';
import { polkadotThresholdSignerKeygenVerificationFailureEvent } from '../../polkadotThresholdSigner/keygenVerificationFailure';
import { bitcoinThresholdSignerKeygenVerificationFailureEvent } from '../../bitcoinThresholdSigner/keygenVerificationFailure';
import { solanaThresholdSignerKeygenVerificationFailureEvent } from '../../solanaThresholdSigner/keygenVerificationFailure';

export const thresholdSignerKeygenVerificationFailureEvent = {
  Arbitrum: evmThresholdSignerKeygenVerificationFailureEvent,
  Assethub: polkadotThresholdSignerKeygenVerificationFailureEvent,
  Bitcoin: bitcoinThresholdSignerKeygenVerificationFailureEvent,
  Ethereum: evmThresholdSignerKeygenVerificationFailureEvent,
  Polkadot: polkadotThresholdSignerKeygenVerificationFailureEvent,
  Solana: solanaThresholdSignerKeygenVerificationFailureEvent,
} as const;
