import { bitcoinThresholdSignerKeygenVerificationSuccessEvent } from '../../bitcoinThresholdSigner/keygenVerificationSuccess';
import { evmThresholdSignerKeygenVerificationSuccessEvent } from '../../evmThresholdSigner/keygenVerificationSuccess';
import { polkadotThresholdSignerKeygenVerificationSuccessEvent } from '../../polkadotThresholdSigner/keygenVerificationSuccess';
import { solanaThresholdSignerKeygenVerificationSuccessEvent } from '../../solanaThresholdSigner/keygenVerificationSuccess';

export const thresholdSignerKeygenVerificationSuccessEvent = {
  Bitcoin: bitcoinThresholdSignerKeygenVerificationSuccessEvent,
  Evm: evmThresholdSignerKeygenVerificationSuccessEvent,
  Polkadot: polkadotThresholdSignerKeygenVerificationSuccessEvent,
  Solana: solanaThresholdSignerKeygenVerificationSuccessEvent,
} as const;
