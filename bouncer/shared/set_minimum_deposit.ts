import { submitGovernanceExtrinsic } from './cf_governance';
import { observeEvent } from './utils/substrate';
import { Asset } from './utils';

export async function setMinimumDeposit(asset: Asset, amount: bigint) {
  const eventHandle = observeEvent('IngressEgress:MinimumDepositSet');
  switch (asset) {
    case 'Btc':
      await submitGovernanceExtrinsic((api) =>
        api.tx.bitcoinIngressEgress.updatePalletConfig([
          { SetMinimumDepositBitcoin: { asset, minimumDeposit: amount } },
        ]),
      );
      break;
    case 'Usdc':
    case 'Usdt':
    case 'Flip':
    case 'Eth':
      await submitGovernanceExtrinsic((api) =>
        api.tx.ethereumIngressEgress.updatePalletConfig([
          { SetMinimumDepositEthereum: { asset, minimumDeposit: amount } },
        ]),
      );
      break;
    case 'ArbUsdc':
    case 'ArbEth':
      await submitGovernanceExtrinsic((api) =>
        api.tx.arbitrumIngressEgress.updatePalletConfig([
          { SetMinimumDepositArbitrum: { asset, minimumDeposit: amount } },
        ]),
      );
      break;
    case 'Dot':
      await submitGovernanceExtrinsic((api) =>
        api.tx.polkadotIngressEgress.updatePalletConfig([
          { SetMinimumDepositPolkadot: { asset, minimumDeposit: amount } },
        ]),
      );
      break;
    case 'SolUsdc':
    case 'Sol':
      await submitGovernanceExtrinsic((api) =>
        api.tx.solanaIngressEgress.updatePalletConfig([
          { SetMinimumDepositSolana: { asset, minimumDeposit: amount } },
        ]),
      );
      break;
    default:
      throw new Error(`Unsupported asset type: ${asset}`);
  }

  await eventHandle.event;
}
