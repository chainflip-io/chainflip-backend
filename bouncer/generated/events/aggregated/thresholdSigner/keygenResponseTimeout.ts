import { evmThresholdSignerKeygenResponseTimeoutEvent } from '../../evmThresholdSigner/keygenResponseTimeout';
import { polkadotThresholdSignerKeygenResponseTimeoutEvent } from '../../polkadotThresholdSigner/keygenResponseTimeout';
import { bitcoinThresholdSignerKeygenResponseTimeoutEvent } from '../../bitcoinThresholdSigner/keygenResponseTimeout';
import { solanaThresholdSignerKeygenResponseTimeoutEvent } from '../../solanaThresholdSigner/keygenResponseTimeout';

export const thresholdSignerKeygenResponseTimeoutEvent = {
  Arbitrum: evmThresholdSignerKeygenResponseTimeoutEvent,
  Assethub: polkadotThresholdSignerKeygenResponseTimeoutEvent,
  Bitcoin: bitcoinThresholdSignerKeygenResponseTimeoutEvent,
  Ethereum: evmThresholdSignerKeygenResponseTimeoutEvent,
  Polkadot: polkadotThresholdSignerKeygenResponseTimeoutEvent,
  Solana: solanaThresholdSignerKeygenResponseTimeoutEvent,
} as const;
