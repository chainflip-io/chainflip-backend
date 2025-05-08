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
  await forceRecoverSolNonce(
    '2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw',
    '9okuUU3MmaQt7u3zXkHo81wPSTQP143B6L1boqWVmvE3',
  );
  await forceRecoverSolNonce(
    'HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo',
    'GJhezrcLUCC2QHzzXCchU2wSs4TL8YeFctq4B4fSaYfg',
  );
  await forceRecoverSolNonce(
    'HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p',
    'HMp6qZzgYpoYssbHQFwr5Niq4yM6fFwyspUAEmsECJB5',
  );
  await forceRecoverSolNonce(
    'HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2',
    '44qwShafkkyfdU2mn3wch6Pa9zwRFyFEH1hNDbVu2gzq',
  );
  await forceRecoverSolNonce(
    'GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM',
    'FhLWRxi4tjVztYvBC78ya4cCjWhrKqeGRYczT5ycsccR',
  );
  await forceRecoverSolNonce(
    'EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn',
    'Akuha5usUuwspTzUB7xJ5tvofe3hvEYALLDqKZbhWLmZ',
  );
  await forceRecoverSolNonce(
    '9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa',
    'F8cBiUtMYSvpju68izRzXCaAxnM5e9JcMqbtz9vEZgQr',
  );
  await forceRecoverSolNonce(
    'J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna',
    'CW97zxidH13rgJeUwTRWCd9tGhazJ2kQ2W6PVeCHNoac',
  );
  await forceRecoverSolNonce(
    'GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55',
    '6K61mjxTBVAqT26XsizSZiAom8pvPgWyyidQMn4gk5oH',
  );
  await forceRecoverSolNonce(
    'AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv',
    'J2pzCnpWbDQMENinD3BEq6n1rxKsHHuykm5UaUowPoqd',
  );
  await forceRecoverSolNonce(
    'BN2vyodNYQQTrx3gtaDAL2UGGVtZwFeF5M8krE5aYYES',
    'J6ottah2YpJz8cR1vzityToDHiLgNPyaLzFgBavekiFK',
  );
  await forceRecoverSolNonce(
    'Gwq9TAQCjbJtdnmtxQa3PbHFfbr6YTUBMDjEP9x2uXnH',
    '7Q8PgQdkxitfhtmgEffp2HNCqD4APwnNqvM2TFVE26oy',
  );
  await forceRecoverSolNonce(
    '3pGbKatko2ckoLEy139McfKiirNgy9brYxieNqFGdN1W',
    'CiQnX2dCYaGDaze7osEVnJDj9qj64cCzXLJBr2RhVu1',
  );
  await forceRecoverSolNonce(
    '9Mcd8BTievK2yTvyiqG9Ft4HfDFf6mjGFBWMnCSRQP8S',
    'J2oxFYQCbTf5mpMX8ZSZYsDX4QT5DdM81qqdaM9GBTxA',
  );
  await forceRecoverSolNonce(
    'AEZG74RoqM6sxf79eTizq5ShB4JTuCkMVwUgtnC8H94z',
    '5FNRtspK97uMk73rfsab2W7Y89nRShKAh7LG2H69V3eJ',
  );
  await forceRecoverSolNonce(
    'APLkgyCWi8DFAMF4KikjTu8YnUG1r7sMjVEfDiaBRZnS',
    '73Bx9FbduHQUoQdoFphTkhmKrMTo7K15e8VTVDce3Hx7',
  );
  await forceRecoverSolNonce(
    '4ShNXTTHvpVt6bQdZTRdyW6yWXDzrPupdMuxajbEoGE4',
    '4UMqKC193JePLT4WwEMd5fVX1Ye7ypFmo8bVDbSHGqGN',
  );
  await forceRecoverSolNonce(
    'FgZp6NJYWw15U51ynfXCfU9vq3eVgDDAHMSfJ8fFBZZ8',
    'HJ5EhviPiuxSj182mdJneVynwbMJTNF7YAWknxnqSokY',
  );
  await forceRecoverSolNonce(
    'ENQ9Mmg87KFLX8ncXRPDBSd7jhKCtPBi8QzAh4rkREgP',
    '82m7ioDix66sCSzTnLfShmtykk5rB1EAFc1CESpCK789',
  );
  await forceRecoverSolNonce(
    'Hhay1UwkzkFUgrGUYuiCvUwv7kErNzAcZnVRQ2fetT7K',
    '3maFK5P7wqCNivjxpRvtULqhi9HWG9FWXq8bD8SzDpwx',
  );
  await forceRecoverSolNonce(
    '2fUVR42opcHgGLrY1eguDXLYfQPHQe9ReJNmRorVt9v8',
    '3661dY2BYKRWBuyv924eHYJeqyRbmBtr4WqRfaKxcKvD',
  );
  await forceRecoverSolNonce(
    'HfKr1wJASkW5UHs8yNWAqMeaYJdp8K2mdYwkbdVRdVrm',
    '9JyYrjrDM6m21fjBGaM5tVycB85FWct4vTFekcZggZqE',
  );
  await forceRecoverSolNonce(
    'DrpYkMpJWkpNqX9yYgQfc3uZrCVYobJ3RbTABcSkHJkM',
    '6Bhq61HwH1eT1Tjh4JVwa6X8CpkF2Tr1URPezD64e5dC',
  );
  await forceRecoverSolNonce(
    'HCXc3o2go1Y2KhfnykLYXEvofLifXTb7GT13w4GsFmGw',
    '8NddLCf966pPp83zi9hdNgCicdnFcbUXaPi79F4HUbtj',
  );
  await forceRecoverSolNonce(
    'FFKYhae4HSnMmA6JJfe8NNtZeySA9yRWLaHzE2jqfhBr',
    'AAE16rCRWbUPzkTHjbvWLApefmxzVCrGo2hruS6a24v8',
  );
  await forceRecoverSolNonce(
    'AaRrJovR9Npna4fuCJ17AB3cJAMzoNDaZymRTbGGzUZm',
    'Hh2ynoKFro1LU1fbLy7NykwCxQiV7SmKQjNnXsPrbfW4',
  );
  await forceRecoverSolNonce(
    '5S8DzBBLvJUeyJccV4DekAK8KJA5PDcjwxRxCvgdyBEi',
    'HYLqpqxfMWVMRGZoPont71dKgJYS6EFWtryqRxjSQoB',
  );
  await forceRecoverSolNonce(
    'Cot1DQZpm859brrre7swrDhTYLj2NJbg3hdMKCHk5zSk',
    'CmukVBuYvBHoLWpVAzdeBhycHR2badj77ou4yab8vFm9',
  );
  await forceRecoverSolNonce(
    '4mfDv7PisvtMhiyGmvD6vxRdVpB842XbUhimAZYxMEn9',
    '9m4KcEZCj6LmWVGqZ173MzviWFnmgqcGka1d9kVNGCmv',
  );
  await forceRecoverSolNonce(
    'BHW7qFCNHTX5QD5yJpT1hn1VM817Ji5ksZqiXMfqGrsj',
    'CQ79zWQ44ZYh2xHZKdYQAYYthQc11SZzgDxXvp1EvVox',
  );
  await forceRecoverSolNonce(
    'EJqZLeaxi2gVsJgQW4nbmxyWJukK25n7jB8qWKoDgWUN',
    '8fzfVsaJECsNww4xSX7597gE5xK92SLa3GH92wQ4buXi',
  );
  await forceRecoverSolNonce(
    'BJqTPWyoqqgzhkLh1pbPh4KWBqg8kCUNzJ81avitSQrm',
    '6hBPFdjve2LLjG2hW9TVNUXSxgqxpvEFYKV5QGChd57e',
  );
  await forceRecoverSolNonce(
    'EkmPmEmSbwm8EDDYtLtaDgcfuLNtW7MbKx5w3FUpaGjv',
    'CwP3uYxRaJue61zoHFVqWMrwT9LuyAGJRN8ugw97Yy9v',
  );
  await forceRecoverSolNonce(
    'CgwtCv8HQ67imnHEkz24TfXfyA2H5jurxcLGxAgDmNQj',
    'FQQpiNCpBVbn8qfqFny7zVEGx1yuASdtcitMDKMJDuRL',
  );
  await forceRecoverSolNonce(
    'zfKsXSxJ4cTpKS7S6aHL1Hy3m1CEjQuySKSwkWvukQX',
    '2vPeCnWgALjwcGBeULr7xwfFGAeQmxXubnoY4FagP8Eq',
  );
  await forceRecoverSolNonce(
    '2VvN1s6txNYyBdKpaC8b6AZKVqUQiQT2Exrpa7ffCgV6',
    '97rPTPHCYXAkJjST27eBuWbWZSufCFh2zz4yE2QfFagA',
  );
  await forceRecoverSolNonce(
    'A2DT1dc4rA1uMry7WCLwoUEQQNjCAsAMkB4X9Lgo88zd',
    '6nRdBXZVPrzRi9cQKinPhcEQPsnwWGJSP6gvGL5ePgwe',
  );
  await forceRecoverSolNonce(
    '9mNBRGfTMLsSsQUn4YZfRDBVXfQ6juEWbNUTwv2ir9gC',
    'BJmyo9zDypyzMR2JZzqeiNsxCdz1fkgct7qHhXnfgEZ1',
  );
  await forceRecoverSolNonce(
    '3jXiydxPx1P7Ggdja5yt384ryLJAW2c8LRGV8PPRT54C',
    '6K4WXoEsX8vM9B84A4BLx7qSm9HgAoWAzWrBhboiKtfN',
  );
  await forceRecoverSolNonce(
    '7ztGR1z28NpYjUaXyrGBzBGu62u1f9H9Pj9UVSKnT3yu',
    '8gV9YjEpCYS95r82ovLjAJBuUhbbaks2tttmyWzFtevs',
  );
  await forceRecoverSolNonce(
    '4GdnDTr5X4eJFHuzTEBLrz3tsREo8rQro7S9YDqrbMZ9',
    'E8VfReTQ2pSXAFLTaXpuJy35fHXSvHbcv1kffkZFGuGY',
  );
  await forceRecoverSolNonce(
    'ALxnH6TBKJPBFRfFZspQkxDjb9nGLUP5oxFFdZNRFgUu',
    'Ea8x8AA5SmdeDu4DpwsBBKHWhap6atj8E2BnXyh9rGN8',
  );
  await forceRecoverSolNonce(
    'Bu3sdWtBh5TJishgK3vneh2zJg1rjLqWN5mFTHxWspwJ',
    'AzGGZEfXbEdvYKLCM1nmQbJurA1VVe7XHgsJXFRAaZQG',
  );
  await forceRecoverSolNonce(
    'GvBbUTE312RXU5iXAcNWt6CuVbfsPs5Nk28D6qvU6NF3',
    'CgWQBvhdd84Qh2oJWK1fXMs4gza41B75HBwLaFUnorDf',
  );
  await forceRecoverSolNonce(
    '2LLct8SsnkW3sD9Gu8CfxmDEjKAWtFXqLvA8ymMyuq8u',
    '8rAi3dYvaFjKr15a2pwzqKbmu7VY67A3sgLTDc8uyMbs',
  );
  await forceRecoverSolNonce(
    'CQ9vUhC3dSa4LyZCpWVpNbXhSn6f7J3NQXWDDvMMk6aW',
    'FRnsjiHi1TNKUvL3wyX82ruELoHp2MgmfmUCvFWmQewK',
  );
  await forceRecoverSolNonce(
    'Cw8GqRmKzCbp7UFfafECC9sf9f936Chgx3BkbSgnXfmU',
    'GAvSuA4bqUnCGGLwTcXArzwp2GCjZvafCShitoRQVZYX',
  );
  await forceRecoverSolNonce(
    'GFJ6m6YdNT1tUfAxyD2BiPSx8gwt3xe4jVAKdtdSUt8W',
    '6jAt8ZifDiBn7ysx31NtT3yacuu8ZGBHidjsgRs1hTrL',
  );
  await forceRecoverSolNonce(
    '7bphTuo5BKs4JJw5WPusCevmnoRk9ocFiB8EGgfwnh4c',
    'AyiLUqWKRSYmLHwjNe5FWFhNMnwFdot1ZhyVauwHhBFT',
  );
  await forceRecoverSolNonce(
    'EFbUq18Mcdi2gGauRzmbNeD5ixaB7EYVk5JZgAF34LoS',
    'EczujXSDC18gU1pSRaeMQu4JtFyuYGNA2UiJa7emMwUs',
  );
}

await runWithTimeoutAndExit(main(), 60);
