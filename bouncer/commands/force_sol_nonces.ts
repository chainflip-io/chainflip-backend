#!/usr/bin/env -S pnpm tsx

import { PublicKey } from '@solana/web3.js';
import { decodeSolAddress, runWithTimeoutAndExit } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';

async function forceRecoverSolNonce(nonceAddress: string, nonceValue: string) {
  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.forceRecoverSolNonce(
      decodeSolAddress(new PublicKey(nonceAddress).toBase58()),
      decodeSolAddress(new PublicKey(nonceValue).toBase58()),
    ),
  );
}

async function main() {
  // TODO: To update values with final nonces
  await forceRecoverSolNonce(
    '2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw',
    '3bqiCT1g42BUtGvAqiQKafc7mpgARb9xN2TPy5zERFbo',
  );
  await forceRecoverSolNonce(
    'HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo',
    '4U2y2p4zAa24PzVZJ5QKqav9N9GKMigjJVbWNfrhC3Je',
  );
  await forceRecoverSolNonce(
    'HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p',
    '8jiG9huoUXBvNFCLUWJdESL7rRq22qjCtgVVz55MX9iA',
  );
  await forceRecoverSolNonce(
    'HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2',
    '6G6CsbEPp91JRLDgt6BohX7MK3ExmLq1Qm67yqnemHYu',
  );
  await forceRecoverSolNonce(
    'GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM',
    '8DHyaCKuxvFGhLDy2kFU84nZ4xun99SfcpzWrUVcyACn',
  );
  await forceRecoverSolNonce(
    'EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn',
    'GeRKgUxBEr3r6urDeiTwyo7X47D3sowJZn9aqbjZsVGE',
  );
  await forceRecoverSolNonce(
    '9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa',
    '6mJwUTywoZE51Ri6RgM21TBy7Ak8j2DktHCgmzYh17Lz',
  );
  await forceRecoverSolNonce(
    'J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna',
    '85ogFrtBCSeBNDPjEMJZnk6bQghENUDcKp6xxordAqtw',
  );
  await forceRecoverSolNonce(
    'GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55',
    '3oMahdLKsDYu7pVTQHBzEkYfP11aqo72XXKDq9Uz2LNj',
  );
  await forceRecoverSolNonce(
    'AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv',
    'FT2c1WyMAeC2X3WaQHR6DmJRwiWKvQduAhiJ34g9M1ii',
  );
  await forceRecoverSolNonce(
    'BN2vyodNYQQTrx3gtaDAL2UGGVtZwFeF5M8krE5aYYES',
    'GrZ21MGdPNfVGpMbC7yFiqNStoRjYi4Hw4pmiqcBnaaj',
  );
  await forceRecoverSolNonce(
    'Gwq9TAQCjbJtdnmtxQa3PbHFfbr6YTUBMDjEP9x2uXnH',
    '3L7PSsX58vXtbZoWoCHpmKfuWGBWgPH7duSPnYW7BKTP',
  );
  await forceRecoverSolNonce(
    '3pGbKatko2ckoLEy139McfKiirNgy9brYxieNqFGdN1W',
    'F7JuJ8RKYWGNfwf63Y9m6GBQFNzpMfMBnPrVT89dQzfV',
  );
  await forceRecoverSolNonce(
    '9Mcd8BTievK2yTvyiqG9Ft4HfDFf6mjGFBWMnCSRQP8S',
    'FZmSB3pDqzE4KdNd8EmBPPpqN8FKgB88DNKXs1L1CmgK',
  );
  await forceRecoverSolNonce(
    'AEZG74RoqM6sxf79eTizq5ShB4JTuCkMVwUgtnC8H94z',
    'D6w3Q65KGGCSVLYBXk8HeyJPd3Wfi7ywqKuQA6WD95Eh',
  );
  await forceRecoverSolNonce(
    'APLkgyCWi8DFAMF4KikjTu8YnUG1r7sMjVEfDiaBRZnS',
    'Fte11ZNRR5tZieLiK7TVmCzWdqfyTktkpjQBo65ji6Rm',
  );
  await forceRecoverSolNonce(
    '4ShNXTTHvpVt6bQdZTRdyW6yWXDzrPupdMuxajbEoGE4',
    '4i8DRRYVMXhAy517pwvTTda9VS6AsD1DVK55rd4rhmSF',
  );
  await forceRecoverSolNonce(
    'FgZp6NJYWw15U51ynfXCfU9vq3eVgDDAHMSfJ8fFBZZ8',
    'BdrBRAQeUym5R7KKFtVZHBLdu5csb9N4bfTj6q9cvPvo',
  );
  await forceRecoverSolNonce(
    'ENQ9Mmg87KFLX8ncXRPDBSd7jhKCtPBi8QzAh4rkREgP',
    '79boPVjqDj49oeM9gekFpvzHi3NbPkqaboJLRW1ebp8S',
  );
  await forceRecoverSolNonce(
    'Hhay1UwkzkFUgrGUYuiCvUwv7kErNzAcZnVRQ2fetT7K',
    '2j3V4yEsLQBFkHAFpYVJE2zSBcn4MZGctdkGYycY7cJr',
  );
  await forceRecoverSolNonce(
    '2fUVR42opcHgGLrY1eguDXLYfQPHQe9ReJNmRorVt9v8',
    'BrcGnjB8iwSo61YDr23Udg5exZ2rrQyUWnjQBdiXgm6Q',
  );
  await forceRecoverSolNonce(
    'HfKr1wJASkW5UHs8yNWAqMeaYJdp8K2mdYwkbdVRdVrm',
    'ARfKJp7fjXwM3TEPiYbYSwB7MXTCn72mWcaJD5YD4JEb',
  );
  await forceRecoverSolNonce(
    'DrpYkMpJWkpNqX9yYgQfc3uZrCVYobJ3RbTABcSkHJkM',
    '8ocFizTc8y47pSiXFVApLZ7A1sNc8qChj6h8XmAvr36D',
  );
  await forceRecoverSolNonce(
    'HCXc3o2go1Y2KhfnykLYXEvofLifXTb7GT13w4GsFmGw',
    'Brrg6v64nU2qEDRV6mUQYmL8oZjJC7sw8MnkeniAv2Un',
  );
  await forceRecoverSolNonce(
    'FFKYhae4HSnMmA6JJfe8NNtZeySA9yRWLaHzE2jqfhBr',
    '4W7BYj7BzZCudnkrUESAcn3SNshwXDNGPWnW1qdLKZRK',
  );
  await forceRecoverSolNonce(
    'AaRrJovR9Npna4fuCJ17AB3cJAMzoNDaZymRTbGGzUZm',
    'H8ozgM2tnY2BrtgUHWtnLDNAsNqtFinx2M1rufFyC8GW',
  );
  await forceRecoverSolNonce(
    '5S8DzBBLvJUeyJccV4DekAK8KJA5PDcjwxRxCvgdyBEi',
    'HUPysNeqUKTgoS4vJ6AVaiKwpxsLprJD5jmcA7yFkhjd',
  );
  await forceRecoverSolNonce(
    'Cot1DQZpm859brrre7swrDhTYLj2NJbg3hdMKCHk5zSk',
    'JBbeFz5NWAZDyaf7baRVWfxHRNzfTt6uLVycabrdqyFr',
  );
  await forceRecoverSolNonce(
    '4mfDv7PisvtMhiyGmvD6vxRdVpB842XbUhimAZYxMEn9',
    '8NsEEoAQZ1jfnwPVubwm3jx3LnwUdBiWgvSqTzkypGwX',
  );
  await forceRecoverSolNonce(
    'BHW7qFCNHTX5QD5yJpT1hn1VM817Ji5ksZqiXMfqGrsj',
    'BU8A5DWHf9imu2FACGcDLvmoFNj6YjQZNVhkGurLHEGq',
  );
  await forceRecoverSolNonce(
    'EJqZLeaxi2gVsJgQW4nbmxyWJukK25n7jB8qWKoDgWUN',
    '55fo5L9j5YarVYautVVuaLnfUTbkoQwhJK22skVTqsaM',
  );
  await forceRecoverSolNonce(
    'BJqTPWyoqqgzhkLh1pbPh4KWBqg8kCUNzJ81avitSQrm',
    'BviTbyREbcX8ENNj3iW143JGTZLF37F2jtRWSbWqvpoc',
  );
  await forceRecoverSolNonce(
    'EkmPmEmSbwm8EDDYtLtaDgcfuLNtW7MbKx5w3FUpaGjv',
    'Bw6PNsg3AgaNkrwmCRVVt4FQ1qMvTLtacvzM4WcHJ2Gn',
  );
  await forceRecoverSolNonce(
    'CgwtCv8HQ67imnHEkz24TfXfyA2H5jurxcLGxAgDmNQj',
    'GCQi8coVrWpiYDg7kr7XFgHgjWjAR1983Q54pKQ373Ak',
  );
  await forceRecoverSolNonce(
    'zfKsXSxJ4cTpKS7S6aHL1Hy3m1CEjQuySKSwkWvukQX',
    '9gESB9ApcxXBKE7Z2qx9gxLC3oXYyjMzE4qTCVhkbtiC',
  );
  await forceRecoverSolNonce(
    '2VvN1s6txNYyBdKpaC8b6AZKVqUQiQT2Exrpa7ffCgV6',
    'J6wsTZ1wUb8XPfiqoZkJp58mat2keh3qh2BrWSTHUrC',
  );
  await forceRecoverSolNonce(
    'A2DT1dc4rA1uMry7WCLwoUEQQNjCAsAMkB4X9Lgo88zd',
    '93ScfMZZCwMqxJAKEc2PRYvBroDoVywFmmhZoiSRp6kb',
  );
  await forceRecoverSolNonce(
    '9mNBRGfTMLsSsQUn4YZfRDBVXfQ6juEWbNUTwv2ir9gC',
    'wbHfqsNRVmATYbvtjeJ2GZzWXK8CiUS9wCawuwXUWSQ',
  );
  await forceRecoverSolNonce(
    '3jXiydxPx1P7Ggdja5yt384ryLJAW2c8LRGV8PPRT54C',
    'J4ijyFp2VeSyVpaxdfaFQsVjAuEeXTzYybzA9KAfpzpZ',
  );
  await forceRecoverSolNonce(
    '7ztGR1z28NpYjUaXyrGBzBGu62u1f9H9Pj9UVSKnT3yu',
    '2rBreiwLCTH8sbBuCcttgPpGkjwvtVYujTHQj9urqqgA',
  );
  await forceRecoverSolNonce(
    '4GdnDTr5X4eJFHuzTEBLrz3tsREo8rQro7S9YDqrbMZ9',
    '3Kpkfz28P7vyGeJTxt15UcsfkqWHBa6DcdtxfFAAxjgf',
  );
  await forceRecoverSolNonce(
    'ALxnH6TBKJPBFRfFZspQkxDjb9nGLUP5oxFFdZNRFgUu',
    '9Qb2PWxkZUV8SXWckWxrmyXq7ykAHz9WMEiCdFBiu9LF',
  );
  await forceRecoverSolNonce(
    'Bu3sdWtBh5TJishgK3vneh2zJg1rjLqWN5mFTHxWspwJ',
    'DJSiZtVdcY82pHUknCEGGWutz82tApuhact8wmPvogvV',
  );
  await forceRecoverSolNonce(
    'GvBbUTE312RXU5iXAcNWt6CuVbfsPs5Nk28D6qvU6NF3',
    '5twVG69gCWidRsicKncB6AuDQssunLukFFW3mWe5xjEt',
  );
  await forceRecoverSolNonce(
    '2LLct8SsnkW3sD9Gu8CfxmDEjKAWtFXqLvA8ymMyuq8u',
    'FzsrqQ6XjjXfUZ7zsrg2n4QpWHPUinh158KkRjJkqfgS',
  );
  await forceRecoverSolNonce(
    'CQ9vUhC3dSa4LyZCpWVpNbXhSn6f7J3NQXWDDvMMk6aW',
    'EqNgQDEUDnmg7mkHQYxkD6Pp3VeDsF6ppWkyk2jKN7K9',
  );
  await forceRecoverSolNonce(
    'Cw8GqRmKzCbp7UFfafECC9sf9f936Chgx3BkbSgnXfmU',
    'B6bodiG9vDL6zfzoY7gaWKBeRD7RyuZ8mSbK4fU9rguy',
  );
  await forceRecoverSolNonce(
    'GFJ6m6YdNT1tUfAxyD2BiPSx8gwt3xe4jVAKdtdSUt8W',
    'Bm37GpK9n83QK9cUaZ6Zrc8TGvSxK2EfJuYCPQEZ2WKb',
  );
  await forceRecoverSolNonce(
    '7bphTuo5BKs4JJw5WPusCevmnoRk9ocFiB8EGgfwnh4c',
    '3r7idtLjppis2HtbwcttUES6h7GejNnBVA1ueB6ijBWE',
  );
  await forceRecoverSolNonce(
    'EFbUq18Mcdi2gGauRzmbNeD5ixaB7EYVk5JZgAF34LoS',
    '4b9CDrda1ngSV86zkDVpAwUy64uCdqNYMpK4MQpxwGWT',
  );
}

await runWithTimeoutAndExit(main(), 60);
