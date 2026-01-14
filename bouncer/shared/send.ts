import Web3 from 'web3';
import { sendDot } from 'shared/send_dot';
import { getRandomBtcClient, sendBtc } from 'shared/send_btc';
import { sendErc20 } from 'shared/send_erc20';
import { sendEvmNative, signAndSendTxEvm } from 'shared/send_evm';
import {
  getContractAddress,
  defaultAssetAmounts,
  amountToFineAmount,
  chainFromAsset,
  getEvmEndpoint,
  assetDecimals,
  Asset,
} from 'shared/utils';
import { approveErc20 } from 'shared/approve_erc20';
import { getCFTesterAbi } from 'shared/contract_interfaces';
import { sendSol } from 'shared/send_sol';
import { sendSolUsdc } from 'shared/send_solusdc';
import { sendHubAsset } from 'shared/send_hubasset';
import { Logger } from 'shared/utils/logger';

const cfTesterAbi = await getCFTesterAbi();

export async function send(
  logger: Logger,
  asset: Asset,
  address: string,
  amount: string = defaultAssetAmounts(asset),
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
): Promise<any> {
  switch (asset) {
    case 'Btc':
      return sendBtc(logger, address, amount, 1, await getRandomBtcClient(logger));
    case 'Eth':
      return sendEvmNative(logger, 'Ethereum', address, amount);
    case 'ArbEth':
      return sendEvmNative(logger, 'Arbitrum', address, amount);
    case 'Dot':
      return sendDot(address, amount);
    case 'Sol':
      return sendSol(logger, address, amount);
    case 'Usdc':
    case 'Usdt':
    case 'Flip': {
      const contractAddress = getContractAddress('Ethereum', asset);
      return sendErc20(logger, 'Ethereum', address, contractAddress, amount);
    }
    case 'ArbUsdc': {
      const contractAddress = getContractAddress('Arbitrum', asset);
      return sendErc20(logger, 'Arbitrum', address, contractAddress, amount);
    }
    case 'SolUsdc':
      return sendSolUsdc(logger, address, amount);
    case 'HubDot':
    case 'HubUsdc':
    case 'HubUsdt':
      return sendHubAsset(logger, asset, address, amount);
    default:
      throw new Error(`Unsupported asset type: ${asset}`);
  }
}

export async function sendViaCfTester(
  logger: Logger,
  asset: Asset,
  toAddress: string,
  amount: string = defaultAssetAmounts(asset),
) {
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
      value = amountToFineAmount(amount, assetDecimals(asset));
      break;
    case 'Usdc':
    case 'Flip': {
      await approveErc20(logger, asset, cfTesterAddress, amount);
      txData = cfTesterContract.methods
        .transferToken(
          toAddress,
          getContractAddress(chain, asset),
          amountToFineAmount(amount, assetDecimals(asset)),
        )
        .encodeABI();
      break;
    }
    default:
      throw new Error(`Unsupported asset type: ${asset}`);
  }

  await signAndSendTxEvm(logger, chain, cfTesterAddress, value, txData);
}
