import { bitcoinThresholdSignerKeygenResponseTimeoutEvent } from '../../bitcoinThresholdSigner/keygenResponseTimeout';
import { evmThresholdSignerKeygenResponseTimeoutEvent } from '../../evmThresholdSigner/keygenResponseTimeout';
import { polkadotThresholdSignerKeygenResponseTimeoutEvent } from '../../polkadotThresholdSigner/keygenResponseTimeout';
import { solanaThresholdSignerKeygenResponseTimeoutEvent } from '../../solanaThresholdSigner/keygenResponseTimeout';

export const thresholdSignerKeygenResponseTimeoutEvent = {
  Bitcoin: bitcoinThresholdSignerKeygenResponseTimeoutEvent,
  Evm: evmThresholdSignerKeygenResponseTimeoutEvent,
  Polkadot: polkadotThresholdSignerKeygenResponseTimeoutEvent,
  Solana: solanaThresholdSignerKeygenResponseTimeoutEvent,
} as const;
