pub use super::common::*;
use super::{parse_account, StateChainEnvironment};
use cf_chains::{btc::BitcoinNetwork, dot::RuntimeVersion, eth::CHAIN_ID_GOERLI};
use cf_primitives::{AccountId, AccountRole, BlockNumber, FlipBalance};
use sc_service::ChainType;
use sp_core::H256;

// *** Overrides from common
pub const ACCRUAL_RATIO: (i32, u32) = (10, 10);
// ***

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Perseverance";
pub const CHAIN_TYPE: ChainType = ChainType::Live;

pub const BITCOIN_NETWORK: BitcoinNetwork = BitcoinNetwork::Testnet;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("9ada116ec46a6a0501bCFFC3E4C027a640a8536e"),
	eth_usdc_address: hex_literal::hex!("07865c6e87b9f70255377e024ace6630c1eaa37f"),
	state_chain_gateway_address: hex_literal::hex!("0e30aFE29222c093aac54E77AD97d49FFA51cc54"),
	key_manager_address: hex_literal::hex!("50E436B37F69b6C4Ef11BfDB62575c1992c49464"),
	eth_vault_address: hex_literal::hex!("53685A9158255dE80FbC91846c0Ae0C5F3070A91"),
	ethereum_chain_id: CHAIN_ID_GOERLI,
	eth_init_agg_key: hex_literal::hex!(
		"0238de34ff83a64fe33bfc888d2736d10f1d0776cd1845382ae345dd3dad6d2f13"
	),
	ethereum_deployment_block: 9184114u64,
	genesis_funding_amount: GENESIS_FUNDING_AMOUNT,
	min_funding: MIN_FUNDING,
	eth_block_safety_margin: eth::BLOCK_SAFETY_MARGIN as u32,
	max_ceremony_stage_duration: 300,
	dot_genesis_hash: H256(hex_literal::hex!(
		"bb5111c1747c9e9774c2e6bd229806fb4d7497af2829782f39b977724e490b5c"
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 9360, transaction_version: 19 },
};

pub const EPOCH_DURATION_BLOCKS: BlockNumber = 24 * HOURS;

pub const BASHFUL_ACCOUNT_ID: &str = "cFLbassb4hwQ9iA7dzdVdyumRqkaXnkdYECrThhmrqjFukdVo";
pub const BASHFUL_SR25519: [u8; 32] =
	hex_literal::hex!["789523326e5f007f7643f14fa9e6bcfaaff9dd217e7e7a384648a46398245d55"];
pub const BASHFUL_ED25519: [u8; 32] =
	hex_literal::hex!["7fdaaa9becf88f9f0a3590bd087ddce9f8d284ccf914c542e4c9f0c0e6440a6a"];
pub const DOC_ACCOUNT_ID: &str = "cFLdocJo3bjT7JbT7R46cA89QfvoitrKr9P3TsMcdkVWeeVLa";
pub const DOC_SR25519: [u8; 32] =
	hex_literal::hex!["7a467c9e1722b35408618a0cffc87c1e8433798e9c5a79339a10d71ede9e9d79"];
pub const DOC_ED25519: [u8; 32] =
	hex_literal::hex!["3489d0b548c5de56c1f3bd679dbabe3b0bff44fb5e7a377931c1c54590de5de6"];
pub const DOPEY_ACCOUNT_ID: &str = "cFLdopvNB7LaiBbJoNdNC26e9Gc1FNJKFtvNZjAmXAAVnzCk4";
pub const DOPEY_SR25519: [u8; 32] =
	hex_literal::hex!["7a4738071f16c71ef3e5d94504d472fdf73228cb6a36e744e0caaf13555c3c01"];
pub const DOPEY_ED25519: [u8; 32] =
	hex_literal::hex!["d9a7e774a58c50062caf081a69556736e62eb0c854461f4485f281f60c53160f"];
pub const SNOW_WHITE_ACCOUNT_ID: &str = "cFLsnoJA2YhAGt9815jPqmzK5esKVyhNAwPoeFmD3PEceE12a";
pub const SNOW_WHITE_SR25519: [u8; 32] =
	hex_literal::hex!["84f131a66e88e3e5f8dce20d413cab3fbb13769a14a4c7b640b7222863ef353d"];

pub fn extra_accounts() -> Vec<(AccountId, AccountRole, FlipBalance, Option<Vec<u8>>)> {
	[
		vec![
			(
				parse_account("cFMTNSQQVfBo2HqtekvhLPfZY764kuJDVFG1EvnnDGYxc3LRW"),
				AccountRole::Broker,
				1_000 * FLIPPERINOS_PER_FLIP,
				Some(b"Chainflip Genesis Broker".to_vec()),
			),
			(
				parse_account("cFN2sr3eDPoyp3G4CAg3EBRMo2VMoYJ7x3rBn51tmXsguYzMX"),
				AccountRole::LiquidityProvider,
				1_000 * FLIPPERINOS_PER_FLIP,
				Some(b"Chainflip Genesis Liquidity Provider".to_vec()),
			),
		],
		phoenix_accounts(),
	]
	.into_iter()
	.flatten()
	.collect()
}

#[ignore = "Only used as a convenience."]
#[test]
fn print_total() {
	let s = phoenix_accounts().iter().map(|(_, _, s, _)| *s).sum::<u128>();
	println!("{s} / {}", s / FLIPPERINOS_PER_FLIP);
}

fn phoenix_accounts() -> Vec<(AccountId, AccountRole, FlipBalance, Option<Vec<u8>>)> {
	[
		(
			"cFJeJxM4y6GJyWdhmRonGvETDY2QbetFbnfAkZvkcRycXXAA2",
			"TrieuNguyen#1801",
			11468557310889288603298,
		),
		(
			"cFPQ2KBhApbCzAqvTaRRTUM6f9U9G1JocS7jyKGp1zqDPG1aG",
			"Yorick | cryptomanufaktur.io#0990 1",
			27863882668944819776315,
		),
		("cFN5nPzBrb6Xfa5BaFZL4J894TJiq5TQ2brxgUWbdbB81hSbW", "ezze#5910", 22944670763275181132946),
		("cFNN6Yv1WJveyqbC7cbZwmyRdtDTP6gS4mnKionzeM8z5MfSV", "bhlee7909", 5967349138381406787293),
		("cFPXgDjGYYBS5iepjsnckzwzeL1TgU1daWjFySpUbgn8LpbpN", "axiline", 999955371094136000000),
		("cFM9UVKfzto11S5NSQ8F3RqfrrhxHSb3WFWtTDYJhwFHz43Kn", "Alexaaa#9814", 19962857690388000000),
		(
			"cFLizejLJgZpk6dopPLbLeD2VSmveBBjw8jMhWtRynfHnNpPr",
			"⍜ Chainflip Validator",
			25589338516721969685738,
		),
		(
			"cFMwZpemYbYoHdgFMx47MHhuSX4zGztxN45feAwrNLc1U1jAw",
			"JETLI111#9983",
			25342777897673034047206,
		),
		(
			"cFNrwPJHB2XSUFRNLzQhx6fdtfwUh6TZqcbW9QonLk5pnHaX4",
			"Lucifer1903",
			22335436231925313629580,
		),
		(
			"cFHtFa7ecdhKkJ47kWrwATmiUHjtp9Y6TVVHJVS9CqKXebnoV",
			"⍜ Chainflip Validator",
			19566972903661520235503,
		),
		("cFLdopvNB7LaiBbJoNdNC26e9Gc1FNJKFtvNZjAmXAAVnzCk4", "dopey", 21430744594105204499021),
		(
			"cFLx7uzuKwb5JwW64CSghgPww1GCJXew87ohYMPTExz7HbnWH",
			"banghj#9194",
			19364883249966479193986,
		),
		("cFKEHcvhggcZy7C2jVZH9PCCdNnLd2ieikGWjzi74wMN66ujQ", "stakr-space", 499972935626250000000),
		("cFME5j9jt48owgpEiYv7VwcUnvGQCga2ECYW5iaiAwHfbZ8dF", "wholis", 328084754061327395392),
		("cFNmcgAjinQ64BFNDHYu4Sm4h6AMF1AWYAk8nMHELhuiDymA9", "ky#4332-2", 12127985826168927493399),
		("cFJyEmN5az9CLGQ3CVGiC1L5oD2iERtLvKNyZjAJgXMmzA9sM", "cryptocat", 19951866767466000000),
		(
			"cFML2ofyLMCRz4T1SgDjCtsa7zv4iMyuhNaxHCXircZYBhjKo",
			"my-discord-username",
			10655599705542686959986,
		),
		("cFKpGGokLRWYgWj3upj5bEFadCnQrwTj5SM7jEXwjFPYVXxGf", "Gas_node", 22062735613050724424),
		(
			"cFNmtvtWe5qaqvpd5L9tBKyspknJKPRXcreCZniGmGTX2yJy5",
			"kernelpanic#9342_4",
			16779924462410555389539,
		),
		(
			"cFJJ9YdUS4idUvXq7k5ZXLVHDMcSaiTHpCEaw8jx3gf7ntM2V",
			"smacktooo#0171",
			28876006715883126715452,
		),
		("cFKWtxfEgdNMajcoG2n72SSHcX8dRWYyzvjstLKVRoKGufNQB", "barad", 17061191319284490363547),
		(
			"cFNaZLWYpQk9NyE9NWii8cDgfZgXmWdpjnw6hwKWNzNkX5iyY",
			"ruslannode",
			15886680483854647022086,
		),
		(
			"cFJCzKfpJtV64QNEqhRdQgpybrNB3DUrTpsZyxyQnvs5TUZDC",
			"Alblock#6022",
			15743974923134572904712,
		),
		("cFNiDku6hjkWNG4rvhj9BawBAg5HidWU7oW68iNntAV8CnYs3", "gxsh", 8294350882072438042003),
		(
			"cFKexUvXf3g5xuVcYZCLnMMAaeD2gjmseD2BVjf6hr2HUdZSV",
			"STAKEME#9529",
			26281717284639223481469,
		),
		("cFMufygKXf6Zy5PRUPvwE8txXb9QaqUyjL9ZApG8sFgPc9Xwv", "toxa", 9044310482673508765884),
		(
			"cFPSU1UF5vKg5sxeV6mQiiT44Bs5nfpnK2jh6a9BdkVbQJGad",
			"book88#8543",
			9318302899664560425183,
		),
		(
			"cFLWpGwoMkavvybJbWS7nqKbC1jJR6UqXnVtrp3L7k9QDfWbf",
			"FizLast#4712_2",
			21404505193045622191269,
		),
		("cFMeRq1XDM5tLaouiZkLan53iieeBcXenV8tp865N4K8nfBZn", "Splashco", 10051587288870000000),
		(
			"cFJQy58CJKJhNCBnV89qQhcQYQSgC6cg8dGWiTJb8xqWsMyQ3",
			"djterminator.eth#2704_1",
			42753637366931351967668,
		),
		("cFN8q7NSPETkMj9vVjha2HKCAFfKQFM8r5occcEUEWRHVpE4q", "SFox#3597", 24941515084454741094753),
		(
			"cFNj4TzwkNn5gpkCA1j6MpuS2jeFAdWQTGHL6YSvrz4YabKRw",
			"sarabana#5147",
			19028499607987306938051,
		),
		("cFPd4Zv4AP6P61aMQiAXz67ddCmcPPrLNNwjSGumiXGBfCc36", "alina", 22807720476596499091),
		("cFKpJARftgrY2xzyy1RJrg9CxeeKyycJpMJ9cJdkzDJnqELLe", "bannyiav", 14340588417768349669),
		("cFJt7m2wWj9mRrqtk9TySiT2WLD8J9U4CADHv6eT6kqk7xfip", "ky#4332-1", 27477613115021846188253),
		(
			"cFJneLgJh16WiD3qoc6FhRdcC9u4xVNJyzPQaEBxynFAJ7zYS",
			"ShpakYevheniy#3528",
			28540141983507989723622,
		),
		("cFL9dwRrQ4AF61x4JckxLTFP5i5rAnVvq48WMY3AxrPabfamE", "xaxaxa7", 18128441340647254255),
		(
			"cFHveKyTwzz7TL1GEP94GmQuqSecBkFc3LCYhnKBK7bXc6ExB",
			"icolbt#7318",
			12283551639820300563160,
		),
		("cFPT6McmqfCATe3seNsTDdzZ2GUVddmf1ydaJLk1ykM85E8Je", "f04ever#5211", 19985117489220076993),
		(
			"cFHyLkLMUvqQE3VLowsDMfh69aHr7auppH9YRJaJKMWZrk5iE",
			"⍜ Chainflip Validator",
			24029283941259430808598,
		),
		(
			"cFMzqt2ef1aPpa1HoV1yoKSuKrk2aQ8mBQSNSXE1YYb29kPcr",
			"SHABLYA#2121",
			34752590030964469989967,
		),
		(
			"cFJNu8hr5RZ2wHnNgCobFiCsN3qpKWYpys6CWg2i87phHv11H",
			"addi#0007 7",
			21007480153289893959077,
		),
		(
			"cFLuHdMyni9xd9ZrToxUfjTVrNmXuReQ5hng1oxERU3h7xnZh",
			"YanisIrving-node",
			9092083397563727918790,
		),
		(
			"cFJALkgsKMcJQAhp9nJ2u56Spe5PNsyA4KiyqUD3jJMPkbjtJ",
			"LUK#3170-2",
			20150587604368522324834,
		),
		(
			"cFMTJCLYFWZ35NdDxfFikXUVkqyU5H4jXTLXFobdeWn9Gv4Y2",
			"Liver#1860",
			22989176777932844960291,
		),
		("cFJa7X523r4thpbcfGY3WSMTZFyZeuY5kBFHBfdNZGE1A7KV4", "dLnodes", 7501005519617828106507),
		(
			"cFPd6AZWENPzVLttBJiQFSf6msffyn3BYizYRJXMAXhEuKV9q",
			"rtx-go#7100",
			31194000105719368502937,
		),
		("cFJcVCPUXF2YGiwC2anGYybTsi6uCaCPV4Mc9swpPET84KK9c", "Vlad", 3926900590477330780785),
		("cFJnKaHhsaA4vRELJioqATQzG7iyt2NyDvZbcFmFxw1fD7PKQ", "toshinaga", 4000055198678732555002),
		(
			"cFL8puWKY5b17pkbicSDLcHuFEphCs8NVSGT2wjQnw3drWu8G",
			"Rex Nader#4226",
			10945954312802928211381,
		),
		(
			"cFLTSe857hxiFmKdwz2CnNz8rmgGyzXgkatXzJF8W28qZGwUi",
			"cAtchfire#0001",
			24306126607270478810830,
		),
		(
			"cFJxf6yRbDaFqh8ybh4pnJdjFhdj56wFpZa61yzEu9FmneSdE",
			"JesCR#3148",
			27583530692083430182092,
		),
		(
			"cFLWD5MwMrGbd5GEctQqZ3vrHAPJZ1ngSNY9t6exKavi4bXuM",
			"icolbn#5588",
			11729694008743066061831,
		),
		(
			"cFMxfJdYcd83r1gW9PtpTZMMqxiLkAbLqqUctWWTsD7NpPhV2",
			"addi#0007 6",
			22123467839436868948949,
		),
		("cFKZR3cb4BJnKbeuA6BNaMeD7h7v9qxDRX94oJBx1upEy6fyn", "alla", 18008119222073502693458),
		("cFJ2Xk4YdE9AwVKdVzY3exkrqx8r7ZQWtj7buREqLWEHTCnJy", "Shoni", 999955998693165000000),
		("cFJHEHB7w7ft7fZQgSJGPCR6U9mjUpoZ9fK8tWnVzBvtrTEeg", "Cabinet42", 12549080627502846675372),
		("cFJF2x4ENfx1iYBnmSvzSW8FR8QL4ssACmwojLdgrd8usWtim", "Mover#6978", 3104022879755783736729),
		(
			"cFLYakBGo5Zau5yPf5yw6pu12au1UQbyfVGRYBiPfjTouQTUw",
			"Zahra.p1400#6173",
			11725370807656625978679,
		),
		(
			"cFLbEU5h6m3HzUY2w4WHNmiREd1VteS55KzfDfwcKiuZmiEu5",
			"v2202211181141207566",
			69831102868571129934,
		),
		(
			"cFMitRmMPgeDFZRT5xSCi5yu5MNqX8j5dA2pZKrTYJLn8GqYx",
			"cryptobtcbuyer",
			7699948646684074000000,
		),
		("cFJC6i1YoP25jgKLrsYhPpDek7fhkYjMrmpMDyhA43wCpAKXj", "alex94", 11737609861218214236241),
		(
			"cFJhBAJa8AVCWfE6ouDGYT9NS7TCSQybcBmZcWiD7ccBachZu",
			"chainflip_timur",
			16448423181949659967,
		),
		(
			"cFLGq6JKWkWQH34cWRfJnGZQ8xjmvnzontx2gsymBHZc7zasC",
			"TreStylez#7381-2",
			18713100224479383844460,
		),
		(
			"cFJUrnuyGxbGh7xuNenFkJittHMVz7kyCbiTw2v6W56tGJZHs",
			"[NODERS]SeptimA#9554",
			18682960924409850842612,
		),
		(
			"cFMRKNkbdTHxjMFm8Kq3QtNyY2GfqLua74k1vGeMkmyDaoCU8",
			"Oleksandr#5702",
			31025251478133864657443,
		),
		(
			"cFNDgSASbiTXFi1P7RJZ2knXU76Wtx6kc9ESM3vnfv1nP5PPG",
			"lixianyun#5735",
			30326553237458905845180,
		),
		("cFKe2Q5ZoQSuz59Mg7UtCE9sMh2MRhhXkdSRicPvWCH9eWBJL", "Velo#2937", 33163446810419797442404),
		("cFJbba3HRnXXM1M1Trwfu5kR56TAUohZ1iPQF4E7p1UVDnQCC", "vinnodes", 4225219710078285865756),
		("cFMg7kgb1PkLXhno7bzidvZ7eTocQda4zi2ZGj8PgT3XX2Jcd", "korovir", 11989795847850574787522),
		("cFN6C8mJDMsM8F7xem4HDLCsctKtXwRaeWRLQPfMqYiW6Shv1", "", 999952158268391000000),
		(
			"cFPMBFu5JeVLCjabdSMj7x5QDBQpQNkKUDjPgWjZbnf1GiFfB",
			"duxxxi#9339",
			33286992399783686252886,
		),
		(
			"cFM5uvD3Sgk445JqdN6gz7hremwFPqRZQBnxYZwZ7bhvdJXFp",
			"teolider#6862",
			18449143025816743810462,
		),
		(
			"cFPXjaU3YTPwF3karc7NzH7XhP4uZsKLEiw35QvEVw5GV8fAv",
			"Mayhem-Nodes",
			15981271332857112181013,
		),
		("cFNDeMeykyMxRXnp2SgzQEiZ8RnLTnweu21e9d8uHW6xcKT6Y", "bombermine", 7280326963420177117623),
		(
			"cFP8nVYXxLWTmvy6DwD1NTHAKN1pbg9nFyKbGZXYdsYVwQeCt",
			"icobongben#4379",
			12494628881610304634089,
		),
		(
			"cFLutGeQzDaP6gWrc5Y6uFWRhW8dcXzEn88JQ7TwHFyz8x9UG",
			"Eugenio#9104_1",
			16517420683192542646616,
		),
		("cFHw8Awt2N88sWUxyDu2RXTSEDzYFj2HdRakT9fGEFeCZ3vwT", "tera2206", 8724832113901746016793),
		(
			"cFKYaFibBqsEM4PemxAW7NKf1cjp7qN23oKeMvh6cFc62oprD",
			"seb160.sol#4725",
			22154456154478329300206,
		),
		(
			"cFNr3iMJosfcs93CN6BFXM4LLc4bcVp6BLNMRJffmfS79RpFv",
			"555_EVGENII",
			5999964613807537000000,
		),
		(
			"cFMrGN4n9ZPMEMAaxcpfQi8vreEf2UsAXt9PHQEQqYwEF7yBC",
			"vladimiras_levinas#3633",
			23265807542996326948434,
		),
		(
			"cFM3bb1qLytQrfS8aHdgkuV43kKae99cbEXBDVh7Tc4ASSurq",
			"LUK#3170-3",
			21150824986075592077043,
		),
		(
			"cFJsPAKnjju4VtUrtgT3huhvqweXxWVdB2RNiQh5tkjpdL2tQ",
			"djterminator.eth#2704_2",
			17967078917649835790610,
		),
		(
			"cFJ48swtFtms4hMzrNyqFz7paUZ375Yyaxvmh8cwDWVVVC2zz",
			"yarnik#5413",
			18299699094982527985789,
		),
		(
			"cFKWJX6PqAFS2b7Nd3ZxPoDNz3fQuCLiW2bpECdgThLyhF2Nk",
			"addi#0007 2",
			14091319177247272896730,
		),
		(
			"cFHxnposUi6rh4M9GnUrs64PNmq1YrrQ8HxoCbd7ZhUWdKYsS",
			"denysk#5292 | Stardust Staking | Node1",
			22618443647541874587852,
		),
		(
			"cFHvPm8GroPKzUqsHDdFtPycwuPXzZaRrESjiZsoNkc5W2a6x",
			"khashipc#5725",
			48755654098473000000,
		),
		(
			"cFLy3zVhA3wCphdY2Ugd9e3nEYASA1PHgRRvDdEUxXjBuv2DM",
			"Sirius.nodes",
			15411697183878102109388,
		),
		(
			"cFK272H53RHP7WFKdZGGJN8hT7Xz9RtT455sFYrqrJKNFVQod",
			"fireday#2716",
			2588213983344579031564,
		),
		(
			"cFNdHmZGsXa3kKHbWmZNi19c3TaXQTCVra8YJb9DhNG475dFd",
			"⍜ Chainflip Validator",
			21869264917653177797851,
		),
		(
			"cFJP73Qo3v3D68ZqCm2NiqqTJqjszqXmb96EJfGUdPzou8hg7",
			"RamaAditya#7184",
			19519208774927289379724,
		),
		(
			"cFLn5viwhRGCsFrwJGNzkrMuVbxSZmMg9skpYKbyZKDQ4CRVV",
			"Baho1#5788",
			12486062885838027644831,
		),
		(
			"cFLbCT6iPwJQ9mPPi3VhX5PQVCh7bogpowdPje5rSE51BqVHf",
			"0xAN | Nodes.Guru#9991",
			14265305332226124182981,
		),
		(
			"cFNogARqADFwEafwnRBqCvXhe7CJdE2WH55bMmmmYNzG2nxSk",
			"DJXtroleChad#05",
			19461417128237920358500,
		),
		("cFN3zvKSCgXDi8ftL1tAtdx2hK5KRSzAXXq3aXgLV7zExcfd1", "agrozold", 19673711005998928113337),
		("cFMYd9oEkQ1ftvpJ8JDTQfYDn77tbU229iGY6iCspM7K3nM74", "Demmich", 9737211284446311735221),
		(
			"cFJP2t24r5y6nQBXnNevP9EWqxbghNAzUUDtPRoCmhZGNGGEW",
			"Viscid#0001 - 2",
			15323564372699178069576,
		),
		(
			"cFNdYwwSpPuEMZhfSBr6iJ5s8dpTAQ3tCzQxTRt17jvKct7Ec",
			"⍜ Chainflip Validator",
			26456395802040001566999,
		),
		(
			"cFNPwJTK4rBHDPhNf6chi63uDzqr2otpaTqCaGHvShLBgYVQu",
			"Firstcome#8103",
			25522220793734855638155,
		),
		(
			"cFJbbkWPe5HRkqUBRa8SALheiw4qh9qwSZCvyuRBQ83L8GL7a",
			"yahaio#6348",
			42727434636890587625013,
		),
		("cFN6dEvqPoSNfKfWb3ZDa7Xicy9Up3FBaBTgub5WbwczdFdWt", "STAVR", 11136602362479552009344),
		(
			"cFJP3oKngTn15MEZdXGrhukoQ1z7aFwH64Xqjorrn1DM1MvSs",
			"⍜ Chainflip Validator",
			26441729796257129826460,
		),
		("cFMYFercViBz8Pjv8Cz3H58wwdJMzhpVHiKsiH4B9A8vVkZbi", "rendal", 6031704467008995359527),
		(
			"cFPXSzMdK9SRtyx4Grssd9oxzP6jSi5muEqNSikyDDm2ufApj",
			"Bitking#3227",
			24866072913402179119732,
		),
		(
			"cFJniJKmaY2rL8d9fySAMNvzCD6byiWXVDJJWeu7RnUjyPmHy",
			"addi#0007 5",
			19760630946841084732721,
		),
		(
			"cFM3gguN1QPH436UqYJaCNh4NDDdYwRWEqEZWhoGHsFRyWtyE",
			"JoharNamak#9344",
			19963848271395000000,
		),
		(
			"cFJk3KfF13BL9k63kwZfpAJLF2EBXgFKWRnpw34jymS6XYmg2",
			"LUK#3170-1",
			15083190982198983617607,
		),
		(
			"cFNgXEHeNemrxDaP878AoVwCsH5t7xHWg8jtnG7HRC7pbRaey",
			"TreStylez#7381-1",
			18439255303786122557553,
		),
		("cFMGyhXcg5U12GSJ3XwwHWxbPwvdUB1v56P8c3FTtNiS8GLGS", "KEMEMTEMBEM", 13976687113837783936),
		(
			"cFNfqs2rwPyWkksg8VEwUukVBAMbmSU5BVBVAT57fQXPmo4UJ",
			"specialfactor#2803",
			3229761788354465000000,
		),
		("cFK18Aa5QZQDxdG298WPoEBkoEfVgsmCsfFViZr63cDW4Vqr4", "sprnodes", 544224797624283835452),
		(
			"cFKL8e3geGS1hsjkZGBcGmLhnHKsK28AiuTKck5i8ueZW5T6s",
			"Rick_Lowe#1366",
			10524057335477107792649,
		),
		("cFKLGLYXGeb2gPj7X7v6X8Hk64ziAM4Nf2WBgPXJErEtn1PsM", "Hydrogen", 22141773216104056472146),
		("cFJJ7PHQmTF1CqzjzP2F3Ehib7itJJFgPtiuXpFKBovxVriyj", "chainflip5", 23696243156732681997),
		(
			"cFLLPVS9HEsaRqWQ9LPeQMFiqKCqo5WrjQ8pCRgpyMRU4BXgo",
			"Yorick | cryptomanufaktur.io#0990 3",
			25430580694469360148076,
		),
		(
			"cFNBU9JYJBKAL2fJbrsDGtuBTxCwJqWTUzzhpkL7EWPCefWhf",
			"v2202211181141207565",
			11808930325481087197,
		),
		("cFP576C2dPqSZ29yVNRT4PqR4ka77kMsHfMNDRfhSPXpNAGn4", "Wave", 13806104635475105878647),
		("cFNBPWm7Bgm7zYaYiQ6fy8WHaw667K2dmA4VjEZvJC344tHVH", "Korben", 11874394027839536693883),
		(
			"cFM19dHuysqAqgBFpLEL4ucgyVPLzmg7ZuDofsvJenvKAKsPT",
			"bakubo#4515",
			3499967550008779000000,
		),
		("cFP7gLs1WhbhRvUrk1MDvsYXhHNo75X3L4xNCsFrbNC91qopa", "ifir3", 25977082536177356581),
		("cFLdtHgnrxKMsEFBCegDtSbF5nUQKLiN2vpocXQnmoHsb952G", "Atomstaking", 9995083716002000000),
		(
			"cFK18iYSZWC6HHBnbEYRSDRgDPCibvXoAY6jdnjNKGqpMMygN",
			"NodeX#0101_2",
			18413380234151310985503,
		),
		(
			"cFK6KW5Uko72eQuyW7X1dewcuKcYxeWECCWLQW7CMKkconVvm",
			"⍜ Chainflip Validator",
			21616023063463192234013,
		),
		("cFJcPej7633TvwkNCzW2AcXi1WZrGFxrMcCR9vp1NLF2TZ4CF", "drogoo", 1629947449344665000000),
		(
			"cFLujV5YcvfzsSoDPYAkqqZdKGgj7kYWoy5k4WQurkz6gNaLu",
			"Hold#5945 (NoFloat)",
			12223006644135306982354,
		),
		("cFL6JM7dGs8XRp4bD5hMByP89w2nvcJ9N8qs4vNrNEiPcNc8b", "Arti4", 2245765195951633219969),
		("cFJWUazuDgCnB1kpWT2HHj7bAb6Jav9VNKouHgMKLjdstvDCY", "entoni_07", 18865550151957541766034),
		(
			"cFP3DCYYndMTyXr9sn7czMsmovfjdy88HChc24S9HLH8i21f9",
			"plateau#8110",
			33604452681943623942262,
		),
		(
			"cFL4ToLA2EetFHHcAcLzA7LChfodAentYmqMKNWFWSxB51q99",
			"Viscid#0001 - 1",
			17564996082723665175717,
		),
		("cFNrJuBmHdqz41uA4xRPtAicYrx82Pay9BGkq251c2kZmXWa6", "Sleipnir_0", 100333327131334783767),
		(
			"cFJkBvT7CjFRvaq21U9RGE8VNiRh7zedFs4QKVVP6JzWXdemz",
			"addi#0007 4",
			23779839568249737337316,
		),
		(
			"cFJt5oFuzj1EGF6sk2kmwm6JtCswsmVaqo5zQBrniFqnTw2kX",
			"st.Game#8485 | node 2",
			15967505180659976985886,
		),
		(
			"cFM6M7hyEU4q6DhPG6EEiDqwz4cJo1NBA7Ly4QmDzp4WDGz5v",
			"ColinkaMalinka#1565",
			27551346620100625653774,
		),
		("cFLWUtkuodZA7AZnqNFStuyRnU9HbFYEKyERJ2qxJokY9ujB4", "KingSuper", 18454839961218604692724),
		(
			"cFPVXBmRqcZXDU6p8mDQut3xSVG6g2wNucodq6LTZYvfGAms2",
			"chainflip_krab",
			9955101043388000000,
		),
		("cFLWYACk2N6Vzdbj2Faxhs41wXCHo3cwTVnMBWJhSdanJ2gk3", "Kratos#3842", 17710161831497964924),
		("cFK3m4NsmFDxL43eJddjXgLcifh5qJR8aWT4P43NTkJVg62JU", "teolider", 14498350276876723466945),
		("cFK254S4MXbtkSt6UnUbvbP3yjyhKLNeeLY5w2c7kBJLX23fK", "SERHIO", 22313526172771099824),
		("cFLdocJo3bjT7JbT7R46cA89QfvoitrKr9P3TsMcdkVWeeVLa", "doc", 21185384281868071438691),
		("cFPPaPvjH11CZ93K8mYaSgDpZce3PqrXoarYEise791cvck8q", "Lizard", 1734335435008338864456),
		("cFJL1kQBVfEcDeN9W9wLWdjbMzygrk9AtGaEC1rpTu8WYBP5C", "liluwei", 92164348467672213812),
		(
			"cFLP21CX2Cnhc8qKi7QX1FsmGJZfSFsbBLCoQ8RRGvhBXojgJ",
			"Pedro00#2039",
			3479001003152379033496,
		),
		(
			"cFMfq89P9mxrEH8wP8BdJLvY7HDjBViGhzrccvnsNEjuwHic8",
			"RamaAditya#7184_4",
			17698303914127004034437,
		),
		(
			"cFJ2GmATNDt9tGuiCeupCEM6wd3zoP3pTyASi6qzmsW71BegK",
			"chainflipmitya4",
			27518853806455259773,
		),
		(
			"cFNQvrRRxgAdZHrQdESVAtFNdwWdcdTrBjFTHo54mrvD2LZj3",
			"Sleipnir_3",
			18920685122570748493700,
		),
		(
			"cFLNHxqP4hmXpTAXeAxr86RP5zveBSEh8bA6c8kX9F74nhzvm",
			"denysk#5292 | Stardust Staking | Node2",
			18781023439365969246438,
		),
		(
			"cFKjjsaXbsuGe3oKrywxQs2j4fmBRRtHZMo98opCPSkytiKFN",
			"Kamilo#3953",
			16581348923616362100228,
		),
		("cFPK99NPuYPhivJ4eXnKt9xJxUtnop8nu2qy1eHoG5i4X8ntL", "fajrmdn", 9985168612799209590800),
		(
			"cFLHb9jFUghMXK5WTvebrpgEdLQ1YvTFgVTJtc2H1EDawRny1",
			"Scorpic-Nodes",
			22364989328692669331,
		),
		("cFPVAB3xQmSyUsphjYg5GoKC3x7818DbFmRaX8vjWRVFoQWvd", "snoopfear", 26938298291820900056919),
		(
			"cFP8WbdDbJWkqpHjsPRxVA6Xabzkt9tCwDw3fz3x9xHR11sQ3",
			"kernelpanic#9342_1",
			16487555936795315211331,
		),
		("cFMeHpPHwvKksraBYuFvgKyUhqCnuf7s29SomPwL8J5uZw5QX", "itrocket", 23068980416517553227883),
		(
			"cFKetat6tb7NDsXQDjtdpnigAYUCA1T3bzr3D1L1rPvgaBPUV",
			"Monivong#9695",
			42708555993120286111586,
		),
		("cFPa17xLF8fSDhET8t2JQRjL1fSTHg7BEqztb4Y6LZub2hQwe", "antonxaa", 13725391136828314252),
		(
			"cFM3h17puYdpru19e4r5UNWEKvtdadjypu5AQNGs353PJgR69",
			"FizLast#4712",
			22464751154669973751664,
		),
		(
			"cFKWWiwMQiX18z2BZoSnvzFnd2p75DpAPkewd6DwgW4GqyQAH",
			"Hummagumma#2612",
			19488587483535711042,
		),
		(
			"cFPE2WSCSGj93Q6jj1f5hL72m4sC8RwYZYDZjYbumgwMCi4Lj",
			"nomad-nodes",
			1224387204039878130884,
		),
		(
			"cFJv6mCxYnPa7pCaAD6u4mqqHuoCwiexp5Uw2rvDUyhU6t7t1",
			"RamaAditya#7184_2",
			17875862070197457817087,
		),
		(
			"cFMXv6cuwCiokczCQHq5bmTbS1hgeBsLaB6hMSADMKiG32VB1",
			"👽Stigga👽https://stigga.org/",
			17691370246328301022109,
		),
		(
			"cFKYfVauE8NKRfKiV4LTpGCWwFDN6CFJydsq4JU3qYGu3vgFe",
			"Demario Kris#7468",
			11106506189520521417243,
		),
		(
			"cFN5sMKx5YgQFhX21joEvoPfh51o4b54uq7BBtHef4fHoXAYG",
			"Mav3rick_iphone#2690",
			16501953841126021412047,
		),
		(
			"cFLRQdC1f6thoceAUNfrSPgPVg5kA8WQHcvUTHbqUYTyrVNLd",
			"Zim#9791_2",
			14443529385809206435813,
		),
		("cFLU41xJEENotTeEizqGZ7u4RDuFTrqFMbrz61ndMLNdhs4oG", "SNK", 9970179689689000000),
		("cFMT9NA5FJ722FJmv1HXh6uyvWtawrd4M44cK8Z9SxTfgwtBw", "navi", 5323158460491434958225),
		(
			"cFMai43w9fYavu69FLkwRQDqyHRRse1u9PWEK5k1hLpNqvBML",
			"ArtRuds#9950",
			25460564309648176833565,
		),
		(
			"cFLxzSjhhLADceGKosoVyMtPu5P96FUdGZ5mjWmv8WWYrG9D1",
			"DanielMoon#9292_1",
			31426151514221407720074,
		),
		("cFNghkafuQKd3q9L5PsUebG5oNQsJgLztDBh1fWvFFMFu16rK", "zackinvest", 9602693029026732305085),
		("cFM9QFknvfuV3jEpFMjpgFtncrBEA2TysEfqvzE74ksVMXD1a", "jordj", 13427682605353489025),
		(
			"cFPM45d1uzp2M57Vp5NW9Hn5G3cArJsnCZkbrXoWYXYnVPfhL",
			"sinascorpion#4740",
			18749418808153835866656,
		),
		("cFM6y49vnP86CtdkX44s5FPSBdbbnWAtTjN3Za6o9YvyK5XMy", "GDIC", 8998908862767176810456),
		(
			"cFJXG37yqBFkwGg5WFrTqVDp49J9AUCtX4zcU3HJYs8PU1Zrc",
			"youngha#9048",
			11908338654657596563657,
		),
		("cFNuqfGLZrofEiJHBbjkB4qYbphy4im9QdNstbHw6xMQD4UuW", "VladRunner", 999971421568426000000),
		("cFJHXfqTSA47ETrip3ZiktFMDvVSCc2yqERtUN7AzKxHgGkMg", "wonderflip", 664810813059590903009),
		(
			"cFMdJbpY5dwGnbJ4b2HD9cLVtVHXxWidq63E5HEFYoMusTCGa",
			"karlo//F5#0291",
			2799994140158808000000,
		),
		("cFK9ZHiExMadjetvg9hz3bBEcySZE1W6NVeZdeDWvxsAffYWT", "mrjw777", 3019955607855917000000),
		("cFLjFsuNL9e2w7VBV3bVWgnxkRMLY6qGcQqYFdcQN39u31xZa", "zxcqwe", 10977952058199000000),
		("cFJtB8gwjdRNtXqj6PvZi9okn1upQPaz9QLZSxxk4G9guM93a", "obolonchik", 7223521161833487924585),
		(
			"cFJRc5sxA1dQQwUqKYxKq4CJbM9tR4e2XtEFNw8Wx9fkV89FF",
			"KeeJef#7335",
			18438502689199850967533,
		),
		("cFLbassb4hwQ9iA7dzdVdyumRqkaXnkdYECrThhmrqjFukdVo", "bashful", 22688664848117008169133),
		(
			"cFK9tUyq2TFogm5LKoha4ciLSMZnBPkVVQVfGvnQRWkfvfAm5",
			"Sleipnir_1",
			18330657270678499015886,
		),
		(
			"cFPJAsVeDginRq7n1hXE7uxjdrKmPQGguPtn32cWwkxVNVXAu",
			"0xVexter_Synclab",
			16912153191200866212353,
		),
		("cFJ7tqE9ajR8KQwfhhCU3cMdWgcwYDrZxZjwoyGpdH21JQJSh", "thanhtamdc", 7627864093976962872546),
		("cFJhtomptTsUrkF2oGxRX6VLkYZTM38Tnezc3vcQXEffyZGze", "mmm", 14256632147572712028835),
		(
			"cFLqA5JPW5QwWKeJ5bhdZmMoGa88HRfY7UFEnaM9gmVikvKdw",
			"iwillbuyyourrugs",
			13833876790762193356389,
		),
		("cFMuAswnvSjMFDE18ktnguPZUwtNjEXM5nCHE9wYRHhoJzcvX", "vadim", 27136527674579943441),
		(
			"cFMum8GAM2vbCQks5fMqU98WbXfEw3LSFLU3g5XcibFQvQ6jV",
			"DJXtroleChad#02",
			23504634837801953148811,
		),
		(
			"cFMgUX4QPjvkUXV8scv5Zhd8ia5SHzzpJ8G1n1F7dXG9J8G3E",
			"Katherineklim11#7857",
			29717211734548496971140,
		),
		(
			"cFPQTrJvjhoLFbv9HrLTTrrycCQTGCycJTEvg4mMqD3EVSZw8",
			"amir.askari#2960",
			30208478546539211676139,
		),
		(
			"cFNitmBzHaZBFTHVXAi1wuqVxZfSeZkqC8rYkthTpYSrVVRAj",
			"⍜ Chainflip Validator",
			22733915754638199035857,
		),
		(
			"cFN3x699WszrCLoznrKFmTtBAk3L8ckKZnM7aeN1UpDc7Zx8q",
			"motralman#0449",
			12302024891330386697288,
		),
		(
			"cFKjDXHXTwkkQwLT2kPgeDui9ei6c4uGczBC24keGAruJpi31",
			"RatedRR#1633",
			16542295937650461434141,
		),
		("cFLiqTPbef3ZfceCWCfZkZK8EJRt99YyAaco5ZAsRBSQCHPm8", "Zim#9791", 22431372729113418689557),
		(
			"cFLWqX35ysqFcCG7BBgHKR4HsSvwbAZc89G3N1dCHwpjD1CCn",
			"iclouist.eth",
			4229087590239638516506,
		),
		(
			"cFPUgnyq91zpbq955eoKYJSEiy1sk6ZWACogVAzrWWqVddVGY",
			"trirazat#7144",
			20086881022374082492548,
		),
		("cFLuxLJGc4SVXsjC3J7wivNoRSahgBg27TVHESzreJX8zgWg1", "Arsenio", 533177937568764055513),
		(
			"cFJbed51ntNNuea3FsXkRf39bah9arRSAKn9KkRQQ132ZJweC",
			"NodeX#0101_1",
			18033251344560870833468,
		),
		(
			"cFJgprg7XxCQZMhRshJEjL7DKGJq3zEWtqbkBDyGkWuFfDcou",
			"Hummagumma#2612",
			27473398505523893791974,
		),
		("cFMpFeHpkCUVSf87vmchRyq6HDC4PtYT6XtsFzofYfmuesziX", "amir", 3008791238031579567547),
		(
			"cFLgZzs1KPaeg2NRniiG91iGYyGroyeNuumwKvvCx3JC7j3XL",
			"ThunderPirate",
			24992700378386339907688,
		),
		(
			"cFKjWCADKRp7Xeb3AQNdBS7FKt9LUznxQwTB2eV4E4D1ebMDf",
			"Eugenio#9104_2",
			20074745482369539347843,
		),
		(
			"cFKR5ohQQ5mBjPynNFNyj4CXY7tEZvTjGNkEFAcBZD7CiQm4n",
			"⍜ Chainflip Validator",
			28129485961994623087984,
		),
		("cFJWM2qCmz2zuzuMGXgmVpi2vjPiHtMdLWs1GtWfrYgsqGBFU", "anton1", 17987754359419526524),
		(
			"cFKj8h6CjpfRnqSnkRxuiZLVsGikdBmqXEy2f5ZWyQTUAuA37",
			"Bodbka#7596",
			3092272870202718078517,
		),
		(
			"cFK8ybMhrVuUmCFLcnyfEzSqjm2YArEihSvjPoHjbo9NVZSZA",
			"Lefey#5237",
			26832773399427020197776,
		),
		(
			"cFKedcpi8m2w726Lp2QB8b6s3qfXBPjXDnFpT1aPH5xenr2gW",
			"AgustinNavioACHIJOANA#4431",
			12849726515119078087417,
		),
		(
			"cFMjfWXmSJpNTA56zVFTgowYb7Abczm2ygZZ9NLALohYnHnK7",
			"Farzinfarachain",
			13219239384344771184333,
		),
		(
			"cFKsmWTRJcbAPaXcewtF9K51Ms1T3SYC7eyM9eJ78PmMhiSoy",
			"DanielMoon#9292_2",
			31619210552103193535236,
		),
		(
			"cFJAEyZiW5dbwBtDkUcFNAEGYe2Y4DeBnUYvgoUJkxkhrZW4F",
			"Vagif#6511",
			31972718232068004711098,
		),
		(
			"cFKsWXTQseLX19w5zi38iTAp9Xs2eHHzjKwheyPcK1KckgP7B",
			"psyman.eth",
			22230071940881446912596,
		),
		(
			"cFMwhbET9mJBYftFtJWZh4XpUcUc5tuyKruaDUfuqLQmjDPiP",
			"whisperit#8145-1",
			19518006839836276409633,
		),
		("cFJ2GCZ1mFbdEh7qJzY7LEwyqM7s16y64jSPHLPmi68XtbaHz", "Gladkyi", 21429733346233798194625),
		("cFK1yYAnEG55Ef6L9Tvzchim3Bcu67SaoWeQodQGSkkz9nuwa", "Milan", 16363323410018072114180),
		("cFJeTec6nZ81729HNebTp9Yb8DfUxfwmKmdvZvimGZnokLrvK", "RoomIT", 17577784227986828567244),
		("cFJjmSGkHZpLEj4G8YeGaZZWJ4fG7WBtWddmyRdJLavEAKZMu", "mexone", 15999980254635215155),
		(
			"cFJLxNZctDC5hz1LRfRRYmGqafKjGABq4njQCjYihaJfsMZfk",
			"addi#0007 8",
			19182396085082564378600,
		),
		(
			"cFKNKELTycoP6DeZ1AZMwFjTbuakKJf2p3NFqsA17JiaQmn9b",
			"vlad.mytsai#7849",
			425960333016740000000,
		),
		(
			"cFNzYmSmxyXScvH46Lwt9yMsCKG5Hhk7ETjUkQFLvrjT8LjfF",
			"Jodie Mante#0310",
			15730755718294709245,
		),
		("cFKNmHwQ48xs6waux4AtGW9tgjpbScyijieMXDSKxFSMzkgw5", "HAZE#6666", 999955217000034000000),
		("cFLNozABkHcR8TFC6nK3p1FnvLYx9TntKaYzhicEg3NcUWPwX", "tRDM", 6931654970650853458001),
		(
			"cFJZfb1vQkP6dxbALFpPuvU1suEoJt1er1PLU3cAGHEYjpKBf",
			"KumzyUpnode",
			18929658056792686716763,
		),
		("cFM6GarYyfDMWSdTci195mf2x9pLFLWzxF4hTVAJSj9x6A3gC", "maliyBEST", 30683892120509659549),
		(
			"cFJh2QgDWXmjVHa2khaVgBQ8HSqpsGeQdswAkdYCYneAWbDPz",
			"Mannuella#1123",
			15386257862911225899,
		),
		(
			"cFJ45Fibsm9Um3HgrfFmobDGs5E3At9nwrTiiBub8uHZ71HyA",
			"RamaAditya#7184_3",
			17702792429050556154392,
		),
		("cFJXPm7eQhjqwpgCmP4GiDCgAMurj2GR3PCfncGcbrcowHtz8", "qdqwqwdqw7", 1499953228345950000000),
		("cFN2nBasc3vKskCaV3rcka7SVtuRRe4BEavMgf4DKt2MD7wSt", "1ce", 15996105381862689919578),
		("cFMTaS8FzmRLEycULGLyDwJ6F753VKtMSForxwmY7G9FZXtD1", "AK3#2821", 28813786950309050497823),
		(
			"cFMrJJTy5UvpWdwQHVmhNP1xhUKqGCLjPpc2NGKatSr7DEfpL",
			"my-discord-username",
			1499957041033104000000,
		),
		(
			"cFKbipiYHbtEegVosVCrVwEMwRSTtDemwECpa7Pkhi6Nq1Sef",
			"Kailyn Kunde#3827",
			11154581525150221069381,
		),
		("cFMTjecPpyLghd9gXvUoynyWo8LQFYGCsKVci9JCPqajeifuk", "WaXTeP", 4000955653160681000000),
		("cFKRgk4U5ZF2K1bJYiVqjgNnbQDkp1AUik2LUCx5hp2dDv68P", "Validatrium", 499954975081795000000),
		(
			"cFLTtKyXaFvnaMhbXRsLKZ54dSGBdvwL9c5CdjAak85MjDVDu",
			"DJXtroleChad#03",
			19519756736153926726976,
		),
		(
			"cFKWv99sfCTWqgLHPwWrtQnwKHDNqTV7ExVcdYAYP3bknLqNT",
			"DJXtroleChad#01",
			20023961798746568341006,
		),
		("cFNQfEEXngTs19nkvXB7u91y3F3SR9ZNTWYaD2sp4bcjM6ysj", "motions#2374", 24994567240307023666),
		(
			"cFK9YHwUJHqrYEo37ia9jnUDmC4dqyWMDShgFRCBBVSNdF6nr",
			"⍜ Chainflip Validator",
			24635397317202438514234,
		),
		("cFJLNGkEmEzLq8ca3CNmLFuKiqxrbxALvaWcfTfX9hir3WBz2", "Beta", 11741713734704022314694),
		("cFLgXCzMjjN29SiPGjriK9LYRprrqkjQVcyt3R4GMXCxEh7wc", "w3coins", 32892040102480491438918),
		(
			"cFJUm3hhuFXXJEee1G8j8KycR1x4KHSTRdA5bmzgm2PFVptpn",
			"Gunner Boyer#8399",
			11243202695859791913801,
		),
		("cFKbf7ZT9w8JZAHbWSAHfqnutab2zsa2L7LJ3WGgvcEshyGan", "Serein", 2519953594504347000000),
		(
			"cFL4FVZdkUZNpVjcU3aAeMP8QV4PkVgCA9FN8QzsBLhfN1oNm",
			"n_shaburov#4023",
			13311277888076754740352,
		),
		(
			"cFMpnotV7jHD6bpWSKNoGmLCdeBgEUeC4isNApGvKjaG3Q2D4",
			"⍜ Chainflip Validator",
			29697890667702628602275,
		),
		("cFP2jTSpvJfEWHedmq1ymq1GtEPPCr3Fz43kNKAkR3P2tWNAH", "", 32048157134191645542),
		("cFP58TCLhVsB5vQ2ZU2NV2DhvTMZyvbfemWJAJBLvYehdqedH", "Yasin#1429", 20632228144506506997),
		("cFHsfa3f9K3swfx3FWz16n9ErdMaFh7eWmg9JEGZ2m8jzCrpg", "skilful", 25977291899555266133),
		(
			"cFKj7czmaZQdrFhaKr4V8a3PsMztneST6qtRd7uSGq3ESfBQ6",
			"rahamidreza#0442",
			11595806487988316032692,
		),
		(
			"cFNmDjGCcE6b3koqcvAjTvxWLqMPMG4iESAvM5aoQeDovXXrP",
			"mahdicivil49#7651",
			1905873028586620621704,
		),
		(
			"cFMg3hUK22bGVXCptBm4aampYBAc8xecU39xWH6RoTTJPq82e",
			"st.Game#8485 | node 1",
			19238685904898809652487,
		),
		(
			"cFPdHkMHtKdJiAu5ikuPWZWZT52MLC9MviZLK2zGKa5jnzWU9",
			"my-discord-username",
			10951195921927423134499,
		),
		(
			"cFKgTwHtMdJqNo6FjDSDV5V41E4qC2QfTyLeH2zT4Wfwocgq3",
			"DJXtroleChad#04",
			18134733961626141592404,
		),
		(
			"cFLeiNsgtWM8t57wobg1JWaXqaCiAaFLGEs9uy4dDznLX3dNq",
			"Nodes_Squad",
			4076105568100248123548,
		),
		("cFPJx5TvAHBndEU4o9SzC9nbQxTpBYeG78M7BgcvJAtWy7DL8", ".D", 12339890736694677960475),
		("cFKZ9NoFD4UvV8zuAfyYrdPcvUNyqKT6YxomKQ4kVjSgVrdjF", "Gamflip", 11765581519322777458631),
		("cFN6ao76kMuBkQvcfwWKz9wZjwh1xdiA3cEX8g6kmBMEwoxUF", "Dikci", 21633536532214456240),
		("cFPKAMsvvohxjcn4uLBadWs4i3LVFw1ieJRdvFuNJzgpLcAye", "maxakyla", 9060789168723717211800),
		(
			"cFNz9MtVKWb1Mn7vxGgTD2LUjiucHUEVWgP9qXHxvn2dU9rLP",
			"z10 | BadBlock#9060",
			19187716836456531702026,
		),
		("cFJZYh3ymEo3Kn38BbvMBwoS5nfUmbpx1BnVXPjqjw8XLYKpC", "CyberAlex", 11979746229457599552899),
		("cFJndVcjUitQaZD7GY5ZW1YAGB8d9cfk4U7G1ZPCB1KU51cMk", "garry#3925", 514652977100925922716),
		(
			"cFJZbg7r4FrZS4UikDn76aFqSx67JR4jep6LyGyXvYvd4PnjV",
			"dancube#8939-blocksteady.xyz",
			13158831862424560672271,
		),
		("cFNYDim2edwvi6sJPkJRKWmHBNzugxKQGGUsPPShosZU3QEeg", "Midora", 999954716320699000000),
		(
			"cFMe2sMF1hz2DmLuYFNsu1EsrXVQ3PGo4g9A83c7vum5pnQPu",
			"WAGMIcapital",
			22537781664564511428233,
		),
		(
			"cFJgp4CmvZfwQ8MxKMUqKanJPatvFHHa18YaCdMUvw2hbCsAG",
			"my-discord-control_nodes#4592",
			72054390784899005419,
		),
		(
			"cFKvBvF3Jwb8vmufe9VaVHUbmK9dNbh9VY7fx2F5gTexnMb1Z",
			"whisperit#8145-2",
			20889641541727708923322,
		),
		(
			"cFM1bRkogVvmsNZKXBsv44anxdYUAH1beYRRkgY6vFiMRaFCs",
			"Sleipnir_2",
			21422193547460852939607,
		),
		("cFLMmpsqjdaLaiVFmaCyS5YtbgFcJvV9Hk322MUf6r4B23oPK", "jackielee", 11871299608814810880606),
		("cFLbwtV3q1NS7Pac4bYoYhKp22du14z7azmQSmR1e7M6NZjhF", "bannatr", 19956504176934000000),
		("cFN5yWCVzQCfWmtKJwJwgQGX6p7NMjj4GAkbLUvRd6WpQ4ssZ", "hamedche", 19416209659737319585076),
		(
			"cFMdKJ8HBy81X9kvCDbduGu2ugCozdSJjRrqrbsQ2qXxXg73i",
			"chainflip_labs",
			1986534745174092206973,
		),
		("cFKHk4jvUrnu8Y47BacG8MQM2cYoqKYiahvk8fKGDX7UR2qNM", "h1ghrisk", 15706810937274123859214),
		(
			"cFLdhc8gTnvsG39iw6mB1Zg1bjNSHNkbUCCcPm4jAGNjSrTvu",
			"Laura Lemke#0022",
			10619696440035231142204,
		),
		("cFNSmfEwY2XBNnkZeNZ2cTPfn9nFN3zheq5ocXntushoHqNkM", "ghotoman", 13662429089214207765986),
		("cFKHckhg2JUuTfFLCRwpquMapXKpSP3N19ChVNTjrLmjssvKm", "agrozold2", 11857844014109535623558),
		(
			"cFJo7EiTMbPvCuQ7NjXS5iSZ6fHDNTBKz95oLsJtd6z2HC3QC",
			"TreStylez#7381-3",
			17335111653278183518793,
		),
		("cFKgS1DhJzqKNGjgvJu6zEx6ma1b4FDK8Ge2uYVUEvUBE2YQj", "Madl", 175719724959160675798),
		(
			"cFP8rhbFJZ1QXq9kqLmRamtsteScB1g2vvK2a7z2s8qLpUjCJ",
			"kernelpanic#9342_2",
			22357971368892394513516,
		),
		(
			"cFPQPM1o7xS7o5fxc62H18Sj9Hhxg15L8jPxm21uw4BoiLioG",
			"kernelpanic#9342_3",
			15322450074813349547687,
		),
		(
			"cFNb5H72LiFvDuqapMJu2eUbf79XV1z1A22rGoE3EnbqYXy4J",
			"Artem_SharkWNT#0245",
			11704716627916392599887,
		),
		(
			"cFLKCnJ63eacRWeGZv86KL7vP9gyDyGGw5TBdpwez2iWfyJSj",
			"taras#7777",
			15734019371123572263407,
		),
		(
			"cFKJ34FGMFFopDik7JoFQxv7ECKhY6e3pF2D1UNs1DRqfSFwU",
			"trandinhcuong#4223",
			12029778000020852255740,
		),
		("cFJ2bbMwPMAh9tXcVQu6SMYkwb9qLbyBQVtWdvQZsss8XUHfV", "p.dao", 17021975235014031629940),
		(
			"cFMT3e8uKtpsY1RPYZCgqekatQugWM9emnrz8ZbHWetPHcvQC",
			"Virusina#6779",
			12186877434717433556952,
		),
		("cFM6R8eeBHXnqVZymfB36C1ngChwQfizHSfUTf7fgkWCzzggQ", "Iwaslike", 1350002906210731637907),
		("cFKc2EcrZ71hhaMYAPPkeRn1oTbMpRDRboEmu7M1vnar4fbfY", "Iren", 549753072088479101067),
		(
			"cFNBRgndrFgPsXAESvttFVfe18Z4EZZMRhhbHChXiHZhSsfvd",
			"Yorick | cryptomanufaktur.io#0990 2",
			26664509993360566273450,
		),
		(
			"cFK9v592x1uC8EYko72Gj8DLno5ohjnF5s8yvY4L77CPFGaKR",
			"pride_man#8120",
			35899493255704590591817,
		),
		("cFL63Y2x4VVEREfVCLACQRGYkpPfhHw5cKXFpAgCMe8wnWQS6", "lordslavik", 9999789607816000000),
		("cFJRVqMyuRp2MXtybvF1wooZK29oVQahYfAxtwD2mmWs2PLvW", "bannyisv", 99956641201264000000),
		("cFNTdnz3BFs6ftgdoRpU51aeW4acTK5mh9SqSB3HprM1wjiFe", "GoodWin", 10991330048361000000),
	]
	.into_iter()
	.map(|(addr, name, balance)| {
		(
			parse_account(addr),
			AccountRole::Validator,
			balance,
			if name.is_empty() { None } else { Some(name.as_bytes().to_vec()) },
		)
	})
	.collect::<Vec<_>>()
}
