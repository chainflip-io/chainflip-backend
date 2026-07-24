import { evmThresholdSignerKeyHandoverResponseTimeoutEvent } from '../../evmThresholdSigner/keyHandoverResponseTimeout';
import { polkadotThresholdSignerKeyHandoverResponseTimeoutEvent } from '../../polkadotThresholdSigner/keyHandoverResponseTimeout';
import { bitcoinThresholdSignerKeyHandoverResponseTimeoutEvent } from '../../bitcoinThresholdSigner/keyHandoverResponseTimeout';
import { solanaThresholdSignerKeyHandoverResponseTimeoutEvent } from '../../solanaThresholdSigner/keyHandoverResponseTimeout';

export const thresholdSignerKeyHandoverResponseTimeoutEvent = {
  Arbitrum: evmThresholdSignerKeyHandoverResponseTimeoutEvent,
  Assethub: polkadotThresholdSignerKeyHandoverResponseTimeoutEvent,
  Bitcoin: bitcoinThresholdSignerKeyHandoverResponseTimeoutEvent,
  Ethereum: evmThresholdSignerKeyHandoverResponseTimeoutEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverResponseTimeoutEvent,
  Solana: solanaThresholdSignerKeyHandoverResponseTimeoutEvent,
} as const;
