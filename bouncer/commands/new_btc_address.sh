#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes one or two arguments
// It will take the provided seed from argument 1, turn it into a new bitcoin address and return the address
// Argument 2 can be used to influence the address type. (P2PKH, P2SH, P2WPKH or P2WSH)
// For example: ./commands/new_btc_address.sh foobar P2PKH
// returns: mhTU7Bz4wv8ESLdB1GdXGs5kE1MBGvdSyb

const sha256 = require('sha256');
const secp256k1 = require('tiny-secp256k1');
const {ECPairFactory} = require('ecpair');
const bitcoin = require('bitcoinjs-lib');
const axios = require('axios');

async function main() {
	const btc_endpoint = process.env.BTC_ENDPOINT || 'http://127.0.0.1:8332';
	const seed = process.argv[2] || '';
	const type = process.argv[3] || 'P2PKH';
	const secret = Buffer.from(sha256(seed, {asBytes: true}));
	const ecc = ECPairFactory(secp256k1);
	const pubkey = ecc.fromPrivateKey(secret).publicKey;
	const network = bitcoin.networks.regtest
	var address;

	switch(type){
	case 'P2PKH':{
		address = bitcoin.payments.p2pkh({pubkey, network}).address;
		break;
	}
	case 'P2SH':{
		const pubkeys = [pubkey];
		const redeem = bitcoin.payments.p2ms({m: 1, pubkeys, network});
		address = bitcoin.payments.p2sh({redeem, network}).address;
		break;
	}
	case 'P2WPKH':{
		address = bitcoin.payments.p2wpkh({pubkey, network}).address;
		break;
	}
	case 'P2WSH':{
		const pubkeys = [pubkey];
		const redeem = bitcoin.payments.p2ms({m: 1, pubkeys, network});
		address = bitcoin.payments.p2wsh({redeem, network}).address;
		break;
	}
	default:
		console.log("Invalid address type requested");
		process.exit(-1);
	}

	const axios_config = {
		headers: {'Content-Type': "text/plain"},
		auth: {username: "flip", password: "flip"}
	};

	const get_descriptor_data = {
		jsonrpc: "1.0",
		id: "1",
		method: "getdescriptorinfo",
		params: ["addr(" + address + ")"]
	};

	var wallet_descriptor;
	await axios.post(btc_endpoint, get_descriptor_data, axios_config).then((res) => {
		wallet_descriptor = res.data.result.descriptor;
	}).catch((err) => {
		console.log(err);
		process.exit(-1);
	});

	const register_address_data = {
		jsonrpc: "1.0",
		id: "1",
		method: "importdescriptors",
		params: [[{desc: wallet_descriptor, timestamp: "now"}]]
	};
	await axios.post(btc_endpoint + "/wallet/watch", register_address_data, axios_config).then((res) => {
		console.log(address);
		process.exit(0);
	}).catch((err) => {
		console.log(err);
		process.exit(-1);
	});
}

main();