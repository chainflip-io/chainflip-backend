#!/usr/bin/env pnpm tsx

// INSTRUCTIONS
//
// This command takes one or two arguments
// It will take the provided seed from argument 1, turn it into a new bitcoin address and return the address
// Argument 2 can be used to influence the address type. (P2PKH, P2SH, P2WPKH or P2WSH)
// For example: ./commands/new_btc_address.ts foobar P2PKH
// returns: mhTU7Bz4wv8ESLdB1GdXGs5kE1MBGvdSyb

import Module from "node:module";

const require = Module.createRequire(import.meta.url);

import { ECPairFactory } from 'ecpair';
import bitcoin from 'bitcoinjs-lib';
import axios from 'axios';
import { sha256 } from '../shared/utils';

async function main(): Promise<void> {
  const btcEndpoint = process.env.BTC_ENDPOINT ?? 'http://127.0.0.1:8332';
  const seed = process.argv[2] ?? '';
  const type = process.argv[3] ?? 'P2PKH';
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
      console.log('Invalid address type requested');
      process.exit(-1);
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

  try {
    await axios.post(btcEndpoint + '/wallet/watch', registerAddressData, axiosConfig);
    console.log(address);
    process.exit(0);
  } catch (err) {
    console.log(err);
    process.exit(-1);
  }
}

await main();
