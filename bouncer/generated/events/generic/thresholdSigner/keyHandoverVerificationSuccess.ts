import { bitcoinThresholdSignerKeyHandoverVerificationSuccessEvent } from '../../bitcoinThresholdSigner/keyHandoverVerificationSuccess';
import { evmThresholdSignerKeyHandoverVerificationSuccessEvent } from '../../evmThresholdSigner/keyHandoverVerificationSuccess';
import { polkadotThresholdSignerKeyHandoverVerificationSuccessEvent } from '../../polkadotThresholdSigner/keyHandoverVerificationSuccess';
import { solanaThresholdSignerKeyHandoverVerificationSuccessEvent } from '../../solanaThresholdSigner/keyHandoverVerificationSuccess';

export const thresholdSignerKeyHandoverVerificationSuccessEvent = {
  Bitcoin: bitcoinThresholdSignerKeyHandoverVerificationSuccessEvent,
  Evm: evmThresholdSignerKeyHandoverVerificationSuccessEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverVerificationSuccessEvent,
  Solana: solanaThresholdSignerKeyHandoverVerificationSuccessEvent,
} as const;
