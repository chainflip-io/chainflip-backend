import { bitcoinThresholdSignerThresholdSignatureSuccessEvent } from '../../bitcoinThresholdSigner/thresholdSignatureSuccess';
import { evmThresholdSignerThresholdSignatureSuccessEvent } from '../../evmThresholdSigner/thresholdSignatureSuccess';
import { polkadotThresholdSignerThresholdSignatureSuccessEvent } from '../../polkadotThresholdSigner/thresholdSignatureSuccess';
import { solanaThresholdSignerThresholdSignatureSuccessEvent } from '../../solanaThresholdSigner/thresholdSignatureSuccess';

export const thresholdSignerThresholdSignatureSuccessEvent = {
  Bitcoin: bitcoinThresholdSignerThresholdSignatureSuccessEvent,
  Evm: evmThresholdSignerThresholdSignatureSuccessEvent,
  Polkadot: polkadotThresholdSignerThresholdSignatureSuccessEvent,
  Solana: solanaThresholdSignerThresholdSignatureSuccessEvent,
} as const;
