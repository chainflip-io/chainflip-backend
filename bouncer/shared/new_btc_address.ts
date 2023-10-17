import Module from 'node:module';

import { ECPairFactory } from 'ecpair';
import bitcoin from 'bitcoinjs-lib';
import axios from 'axios';
import { sha256, btcClientMutex } from '../shared/utils';

const require = Module.createRequire(import.meta.url);

export const btcAddressTypes = ['P2PKH', 'P2SH', 'P2WPKH', 'P2WSH'] as const;
export type BtcAddressType = (typeof btcAddressTypes)[number];

export const isValidBtcAddressType = (type: string): type is BtcAddressType =>
  // unfortunately, we need to cast to any here
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  btcAddressTypes.includes(type as any);

export async function newBtcAddress(seed: string, type: BtcAddressType): Promise<string> {
  const btcEndpoint = process.env.BTC_ENDPOINT ?? 'http://127.0.0.1:8332';

  const secret = sha256(seed);
  const eccpf = ECPairFactory(require('tiny-secp256k1'));
  const pubkey = eccpf.fromPrivateKey(secret).publicKey;
  const network = bitcoin.networks.regtest;
  let address: string | undefined;

  switch (type) {
    case 'P2PKH': {
      address = bitcoin.payments.p2pkh({ pubkey, network }).address as string;
      break;
    }
    case 'P2SH': {
      const pubkeys = [pubkey];
      const redeem = bitcoin.payments.p2ms({ m: 1, pubkeys, network });
      address = bitcoin.payments.p2sh({ redeem, network }).address as string;
      break;
    }
    case 'P2WPKH': {
      address = bitcoin.payments.p2wpkh({ pubkey, network }).address as string;
      break;
    }
    case 'P2WSH': {
      const pubkeys = [pubkey];
      const redeem = bitcoin.payments.p2ms({ m: 1, pubkeys, network });
      address = bitcoin.payments.p2wsh({ redeem, network }).address as string;
      break;
    }
    default:
      throw new Error('Invalid address type requested');
  }

  const axiosConfig = {
    headers: { 'Content-Type': 'text/plain' },
    auth: { username: 'flip', password: 'flip' },
  };

  const getDescriptorData = {
    jsonrpc: '1.0',
    id: '1',
    method: 'getdescriptorinfo',
    params: ['addr(' + address + ')'],
  };

  let walletDescriptor;
  try {
    walletDescriptor = (await axios.post(btcEndpoint, getDescriptorData, axiosConfig)).data.result
      .descriptor;
  } catch (err) {
    console.log(err);
    process.exit(-1);
  }

  const registerAddressData = {
    jsonrpc: '1.0',
    id: '1',
    method: 'importdescriptors',
    params: [[{ desc: walletDescriptor, timestamp: 'now' }]],
  };

  // Would get a "Wallet is currently rescanning error"
  // if this is called concurrently
  await btcClientMutex.runExclusive(async () => {
    await axios.post(btcEndpoint + '/wallet/watch', registerAddressData, axiosConfig);
  });

  return address;
}
