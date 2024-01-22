import Web3 from 'web3';
import { Asset, assetDecimals } from '@chainflip-io/cli';
import { sendDot } from './send_dot';
import { sendBtc } from './send_btc';
import { sendErc20 } from './send_erc20';
import { sendEth, signAndSendTxEth } from './send_eth';
import { getEthContractAddress, defaultAssetAmounts, amountToFineAmount } from './utils';
import { approveErc20 } from './approve_erc20';
import { getCFTesterAbi } from './eth_abis';

const cfTesterAbi = await getCFTesterAbi();

export async function send(asset: Asset, address: string, amount?: string, log = true) {
  switch (asset) {
    case 'BTC':
      await sendBtc(address, amount ?? defaultAssetAmounts(asset));
      break;
    case 'ETH':
      await sendEth(address, amount ?? defaultAssetAmounts(asset), log);
      break;
    case 'DOT':
      await sendDot(address, amount ?? defaultAssetAmounts(asset));
      break;
    case 'USDC': {
      const contractAddress = getEthContractAddress(asset);
      await sendErc20(address, contractAddress, amount ?? defaultAssetAmounts(asset), log);
      break;
    }
    case 'FLIP': {
      const contractAddress = getEthContractAddress(asset);
      await sendErc20(address, contractAddress, amount ?? defaultAssetAmounts(asset), log);
      break;
    }
    default:
      throw new Error(`Unsupported asset type: ${asset}`);
  }
}

export async function sendViaCfTester(asset: Asset, toAddress: string, amount?: string) {
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
  const web3 = new Web3(ethEndpoint);

  const cfTesterAddress = getEthContractAddress('CFTESTER');
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const cfTesterContract = new web3.eth.Contract(cfTesterAbi as any, cfTesterAddress);

  let txData;
  let value = '0';
  switch (asset) {
    case 'ETH':
      txData = cfTesterContract.methods.transferEth(toAddress).encodeABI();
      value = amountToFineAmount(amount ?? defaultAssetAmounts(asset), assetDecimals[asset]);
      break;
    case 'USDC':
    case 'FLIP': {
      await approveErc20(asset, cfTesterAddress, amount ?? defaultAssetAmounts(asset));
      txData = cfTesterContract.methods
        .transferToken(
          toAddress,
          getEthContractAddress(asset),
          amountToFineAmount(amount ?? defaultAssetAmounts(asset), assetDecimals[asset]),
        )
        .encodeABI();
      break;
    }
    default:
      throw new Error(`Unsupported asset type: ${asset}`);
  }

  await signAndSendTxEth(cfTesterAddress, value, txData);
}
