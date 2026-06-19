import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { observeEvent } from 'shared/utils/substrate';
import { Asset } from 'shared/utils';
import { Logger } from 'shared/utils/logger';

export async function setMinimumDeposit(logger: Logger, asset: Asset, amount: bigint) {
  const eventHandle = observeEvent(logger, 'IngressEgress:PalletConfigUpdated');
  switch (asset) {
    case 'Btc':
      await submitGovernanceExtrinsic((api) =>
        api.tx.bitcoinIngressEgress.updatePalletConfig([
          { type: 'SetMinimumDepositBitcoin', value: { asset, minimumDeposit: amount } },
        ]),
      );
      break;
    case 'Usdc':
    case 'Usdt':
    case 'Wbtc':
    case 'Flip':
    case 'Eth':
      await submitGovernanceExtrinsic((api) =>
        api.tx.ethereumIngressEgress.updatePalletConfig([
          { type: 'SetMinimumDepositEthereum', value: { asset, minimumDeposit: amount } },
        ]),
      );
      break;
    case 'ArbUsdc':
    case 'ArbUsdt':
    case 'ArbEth':
      await submitGovernanceExtrinsic((api) =>
        api.tx.arbitrumIngressEgress.updatePalletConfig([
          { type: 'SetMinimumDepositArbitrum', value: { asset, minimumDeposit: amount } },
        ]),
      );
      break;
    case 'HubDot':
      await submitGovernanceExtrinsic((api) =>
        api.tx.assethubIngressEgress.updatePalletConfig([
          { type: 'SetMinimumDepositAssethub', value: { asset, minimumDeposit: amount } },
        ]),
      );
      break;
    case 'SolUsdc':
    case 'SolUsdt':
    case 'Sol':
      await submitGovernanceExtrinsic((api) =>
        api.tx.solanaIngressEgress.updatePalletConfig([
          { type: 'SetMinimumDepositSolana', value: { asset, minimumDeposit: amount } },
        ]),
      );
      break;
    default:
      throw new Error(`Unsupported asset type: ${asset}`);
  }

  await eventHandle.event;
}
