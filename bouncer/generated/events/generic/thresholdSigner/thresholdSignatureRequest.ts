import { bitcoinThresholdSignerThresholdSignatureRequestEvent } from '../../bitcoinThresholdSigner/thresholdSignatureRequest';
import { evmThresholdSignerThresholdSignatureRequestEvent } from '../../evmThresholdSigner/thresholdSignatureRequest';
import { polkadotThresholdSignerThresholdSignatureRequestEvent } from '../../polkadotThresholdSigner/thresholdSignatureRequest';
import { solanaThresholdSignerThresholdSignatureRequestEvent } from '../../solanaThresholdSigner/thresholdSignatureRequest';

export const thresholdSignerThresholdSignatureRequestEvent = {
  Bitcoin: bitcoinThresholdSignerThresholdSignatureRequestEvent,
  Evm: evmThresholdSignerThresholdSignatureRequestEvent,
  Polkadot: polkadotThresholdSignerThresholdSignatureRequestEvent,
  Solana: solanaThresholdSignerThresholdSignatureRequestEvent,
} as const;
