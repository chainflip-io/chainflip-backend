import { bitcoinThresholdSignerThresholdSignatureFailedEvent } from '../../bitcoinThresholdSigner/thresholdSignatureFailed';
import { evmThresholdSignerThresholdSignatureFailedEvent } from '../../evmThresholdSigner/thresholdSignatureFailed';
import { polkadotThresholdSignerThresholdSignatureFailedEvent } from '../../polkadotThresholdSigner/thresholdSignatureFailed';
import { solanaThresholdSignerThresholdSignatureFailedEvent } from '../../solanaThresholdSigner/thresholdSignatureFailed';

export const thresholdSignerThresholdSignatureFailedEvent = {
  Bitcoin: bitcoinThresholdSignerThresholdSignatureFailedEvent,
  Evm: evmThresholdSignerThresholdSignatureFailedEvent,
  Polkadot: polkadotThresholdSignerThresholdSignatureFailedEvent,
  Solana: solanaThresholdSignerThresholdSignatureFailedEvent,
} as const;
