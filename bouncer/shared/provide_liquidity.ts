import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { Asset, assetChains, chainContractIds } from '@chainflip-io/cli';
import {
  observeEvent,
  getAddress,
  getChainflipApi,
  encodeDotAddressForContract,
  handleSubstrateError,
  encodeBtcAddressForContract,
  lpMutex,
} from '../shared/utils';
import { send } from '../shared/send';

const chain = new Map<Asset, string>([
  ['DOT', 'dot'],
  ['ETH', 'eth'],
  ['BTC', 'btc'],
  ['USDC', 'eth'],
  ['FLIP', 'eth'],
]);

export async function provideLiquidity(ccy: Asset, amount: number) {
  const chainflip = await getChainflipApi(process.env.CF_NODE_ENDPOINT);
  await cryptoWaitReady();

  const keyring = new Keyring({ type: 'sr25519' });
  const lpUri = process.env.LP_URI || '//LP_1';
  const lp = keyring.createFromUri(lpUri);

  // If no emergency withdrawal address is registered, then do that now
  if (
    (
      await chainflip.query.liquidityProvider.emergencyWithdrawalAddress(
        lp.address,
        chainContractIds[assetChains[ccy]],
      )
    ).toJSON() === null
  ) {
    let emergencyAddress = await getAddress(ccy, 'LP_1');
    emergencyAddress =
      ccy === 'DOT' ? encodeDotAddressForContract(emergencyAddress) : emergencyAddress;

    console.log('Registering Emergency Withdrawal Address: ' + emergencyAddress);
    await lpMutex.runExclusive(async () => {
      await chainflip.tx.liquidityProvider
        .registerEmergencyWithdrawalAddress({ [chain.get(ccy)!]: emergencyAddress })
        .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
    });
  }

  console.log('Requesting ' + ccy + ' deposit address');
  let eventHandle =
    chain.get(ccy) === 'eth'
      ? observeEvent(
          'ethereumIngressEgress:StartWitnessing',
          chainflip,
          (data) => data[1].toUpperCase() === ccy,
        )
      : observeEvent(
          'liquidityProvider:LiquidityDepositAddressReady',
          chainflip,
          (data) => data[1][chain.get(ccy)!] !== undefined,
        );
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .requestLiquidityDepositAddress(ccy.toLowerCase())
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  let ingressAddress =
    chain.get(ccy) === 'eth'
      ? (await eventHandle).depositAddress.toJSON()
      : (await eventHandle).depositAddress.toJSON()[chain.get(ccy)!];
  if (ccy === 'BTC') {
    ingressAddress = encodeBtcAddressForContract(ingressAddress);
  }
  console.log('Received ' + ccy + ' address: ' + ingressAddress);
  console.log('Sending ' + amount + ' ' + ccy + ' to ' + ingressAddress);
  eventHandle = observeEvent(
    'liquidityProvider:AccountCredited',
    chainflip,
    (data) => data[1].toUpperCase() === ccy,
  );
  send(ccy, ingressAddress, String(amount));
  await eventHandle;
}
