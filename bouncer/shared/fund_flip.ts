import type { HexString } from '@polkadot/util/types';
import { fundStateChainAccount } from '@chainflip/cli';
import { Wallet, ethers } from 'ethers';
import { getNextEvmNonce } from './send_evm';
import {
  getContractAddress,
  hexPubkeyToFlipAddress,
  decodeFlipAddressForContract,
  getEvmEndpoint,
  getWhaleKey,
  assetDecimals,
} from './utils';
import { amountToFineAmount } from '../shared/utils';
import { approveErc20 } from './approve_erc20';
import { observeEvent } from './utils/substrate';
import { Logger } from './utils/logger';

export async function fundFlip(logger: Logger, scAddress: string, flipAmount: string) {
  // Doing effectively infinite approvals to prevent race conditions between tests
  await approveErc20(
    logger,
    'Flip',
    getContractAddress('Ethereum', 'GATEWAY'),
    '100000000000000000000000000',
  );

  const flipperinoAmount = amountToFineAmount(flipAmount, assetDecimals('Flip'));

  const flipContractAddress = getContractAddress('Ethereum', 'Flip');

  const gatewayContractAddress = getContractAddress('Ethereum', 'GATEWAY');

  const whaleKey = getWhaleKey('Ethereum');
  logger.debug('Approving ' + flipAmount + ' Flip to State Chain Gateway');

  const wallet = new Wallet(whaleKey, ethers.getDefaultProvider(getEvmEndpoint('Ethereum')));

  const networkOptions = {
    signer: wallet,
    network: 'localnet',
    stateChainGatewayContractAddress: gatewayContractAddress,
    flipContractAddress,
  } as const;
  const txOptions = {
    nonce: await getNextEvmNonce('Ethereum'),
  } as const;

  logger.debug('Funding ' + flipAmount + ' Flip to ' + scAddress);
  let pubkey = decodeFlipAddressForContract(scAddress);

  if (pubkey.substr(0, 2) !== '0x') {
    pubkey = '0x' + pubkey;
  }
  const receipt2 = await fundStateChainAccount(
    pubkey as HexString,
    BigInt(flipperinoAmount),
    networkOptions,
    txOptions,
  );

  logger.debug(
    'Transaction complete, tx_hash: ' +
      receipt2.hash +
      ' blockNumber: ' +
      receipt2.blockNumber +
      ' blockHash: ' +
      receipt2.blockHash,
  );
  await observeEvent(logger, 'funding:Funded', {
    test: (event) => hexPubkeyToFlipAddress(pubkey) === event.data.accountId,
  }).event;
}
