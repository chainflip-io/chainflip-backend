import Web3 from 'web3';
import { sendDot } from './send_dot';
import { sendBtc } from './send_btc';
import { sendErc20 } from './send_erc20';
import { sendEvmNative, signAndSendTxEvm } from './send_evm';
import {
  getContractAddress,
  defaultAssetAmounts,
  amountToFineAmount,
  chainFromAsset,
  getEvmEndpoint,
  assetDecimals,
  Asset,
} from './utils';
import { approveErc20 } from './approve_erc20';
import { getCFTesterAbi } from './contract_interfaces';
import { sendSol } from './send_sol';
import { sendSolUsdc } from './send_solusdc';

const cfTesterAbi = await getCFTesterAbi();

export async function send(asset: Asset, address: string, amount?: string, log = true) {
  // TODO: Remove this any when we have Sol assets in the Asset type.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  switch (asset as any) {
    case 'Btc':
      await sendBtc(address, amount ?? defaultAssetAmounts(asset));
      break;
    case 'Eth':
      await sendEvmNative('Ethereum', address, amount ?? defaultAssetAmounts(asset), log);
      break;
    case 'ArbEth':
      await sendEvmNative('Arbitrum', address, amount ?? defaultAssetAmounts(asset), log);
      break;
    case 'Dot':
      await sendDot(address, amount ?? defaultAssetAmounts(asset));
      break;
    case 'Sol':
      await sendSol(address, amount ?? defaultAssetAmounts(asset));
      break;
    case 'Usdc':
    case 'Usdt':
    case 'Flip': {
      const contractAddress = getContractAddress('Ethereum', asset);
      await sendErc20(
        'Ethereum',
        address,
        contractAddress,
        amount ?? defaultAssetAmounts(asset),
        log,
      );
      break;
    }
    case 'ArbUsdc': {
      const contractAddress = getContractAddress('Arbitrum', asset);
      await sendErc20(
        'Arbitrum',
        address,
        contractAddress,
        amount ?? defaultAssetAmounts(asset),
        log,
      );
      break;
    }
    case 'SolUsdc':
      await sendSolUsdc(address, amount ?? defaultAssetAmounts(asset));
      break;
    default:
      throw new Error(`Unsupported asset type: ${asset}`);
  }
}

export async function sendViaCfTester(asset: Asset, toAddress: string, amount?: string) {
  const chain = chainFromAsset(asset);

  const web3 = new Web3(getEvmEndpoint(chain));

  const cfTesterAddress = getContractAddress(chain, 'CFTESTER');
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const cfTesterContract = new web3.eth.Contract(cfTesterAbi as any, cfTesterAddress);

  let txData;
  let value = '0';
  switch (asset) {
    case 'Eth':
      txData = cfTesterContract.methods.transferEth(toAddress).encodeABI();
      value = amountToFineAmount(amount ?? defaultAssetAmounts(asset), assetDecimals(asset));
      break;
    case 'Usdc':
    case 'Flip': {
      await approveErc20(asset, cfTesterAddress, amount ?? defaultAssetAmounts(asset));
      txData = cfTesterContract.methods
        .transferToken(
          toAddress,
          getContractAddress(chain, asset),
          amountToFineAmount(amount ?? defaultAssetAmounts(asset), assetDecimals(asset)),
        )
        .encodeABI();
      break;
    }
    default:
      throw new Error(`Unsupported asset type: ${asset}`);
  }

  await signAndSendTxEvm(chain, cfTesterAddress, value, txData);
}
