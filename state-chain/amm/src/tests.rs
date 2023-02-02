use crate::*;

mod pools;
mod swapping;
mod tick_spacing;

fn encodedprice1_1() -> U256 {
	U256::from_dec_str("79228162514264337593543950336").unwrap()
}
fn encodedprice2_1() -> U256 {
	U256::from_dec_str("112045541949572287496682733568").unwrap()
}
fn encodedprice121_100() -> U256 {
	U256::from_dec_str("87150978765690771352898345369").unwrap()
}
fn expandto18decimals(amount: u128) -> U256 {
	U256::from(amount) * U256::from(10).pow(U256::from_dec_str("18").unwrap())
}

#[test]
fn max_liquidity() {
	// Note a tick's liquidity_delta.abs() must be less than or equal to its gross liquidity,
	// and therefore <= MAX_TICK_GROSS_LIQUIDITY Also note that the total of all tick's deltas
	// must be zero. So the maximum possible liquidity is MAX_TICK_GROSS_LIQUIDITY * ((1 +
	// MAX_TICK - MIN_TICK) / 2) The divide by 2 comes from the fact that if for example all the
	// ticks from MIN_TICK to an including -1 had deltas of MAX_TICK_GROSS_LIQUIDITY, all the
	// other tick's deltas would need to be negative or zero to satisfy the requirement that the
	// sum of all deltas is zero. Importantly this means the current_liquidity can be
	// represented as a i128 as the maximum liquidity is less than half the maximum u128
	assert!(
		MAX_TICK_GROSS_LIQUIDITY
			.checked_mul((1 + MAX_TICK - MIN_TICK) as u128 / 2)
			.unwrap() < i128::MAX as u128
	);
}

#[test]
fn output_amounts_bounded() {
	// Note these values are significant over-estimates of the maximum output amount
	Asset1ToAsset0::output_amount_delta_floor(
		PoolState::sqrt_price_at_tick(MIN_TICK),
		PoolState::sqrt_price_at_tick(MAX_TICK),
		MAX_TICK_GROSS_LIQUIDITY,
	)
	.checked_mul((1 + MAX_TICK - MIN_TICK).into())
	.unwrap();
	Asset0ToAsset1::output_amount_delta_floor(
		PoolState::sqrt_price_at_tick(MAX_TICK),
		PoolState::sqrt_price_at_tick(MIN_TICK),
		MAX_TICK_GROSS_LIQUIDITY,
	)
	.checked_mul((1 + MAX_TICK - MIN_TICK).into())
	.unwrap();
}

#[test]
fn test_sqrt_price_at_tick() {
	assert_eq!(PoolState::sqrt_price_at_tick(MIN_TICK), MIN_SQRT_PRICE);
	assert_eq!(
		PoolState::sqrt_price_at_tick(-738203),
		U256::from_dec_str("7409801140451").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(-500000),
		U256::from_dec_str("1101692437043807371").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(-250000),
		U256::from_dec_str("295440463448801648376846").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(-150000),
		U256::from_dec_str("43836292794701720435367485").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(-50000),
		U256::from_dec_str("6504256538020985011912221507").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(-5000),
		U256::from_dec_str("61703726247759831737814779831").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(-4000),
		U256::from_dec_str("64867181785621769311890333195").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(-3000),
		U256::from_dec_str("68192822843687888778582228483").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(-2500),
		U256::from_dec_str("69919044979842180277688105136").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(-1000),
		U256::from_dec_str("75364347830767020784054125655").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(-500),
		U256::from_dec_str("77272108795590369356373805297").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(-250),
		U256::from_dec_str("78244023372248365697264290337").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(-100),
		U256::from_dec_str("78833030112140176575862854579").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(-50),
		U256::from_dec_str("79030349367926598376800521322").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(50),
		U256::from_dec_str("79426470787362580746886972461").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(100),
		U256::from_dec_str("79625275426524748796330556128").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(250),
		U256::from_dec_str("80224679980005306637834519095").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(500),
		U256::from_dec_str("81233731461783161732293370115").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(1000),
		U256::from_dec_str("83290069058676223003182343270").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(2500),
		U256::from_dec_str("89776708723587163891445672585").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(3000),
		U256::from_dec_str("92049301871182272007977902845").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(4000),
		U256::from_dec_str("96768528593268422080558758223").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(5000),
		U256::from_dec_str("101729702841318637793976746270").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(50000),
		U256::from_dec_str("965075977353221155028623082916").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(150000),
		U256::from_dec_str("143194173941309278083010301478497").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(250000),
		U256::from_dec_str("21246587762933397357449903968194344").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(500000),
		U256::from_dec_str("5697689776495288729098254600827762987878").unwrap()
	);
	assert_eq!(
		PoolState::sqrt_price_at_tick(738203),
		U256::from_dec_str("847134979253254120489401328389043031315994541").unwrap()
	);
	assert_eq!(PoolState::sqrt_price_at_tick(MAX_TICK), MAX_SQRT_PRICE);
}

#[test]
fn test_tick_at_sqrt_price() {
	assert_eq!(PoolState::tick_at_sqrt_price(MIN_SQRT_PRICE), MIN_TICK);
	assert_eq!(
		PoolState::tick_at_sqrt_price(U256::from_dec_str("79228162514264337593543").unwrap()),
		-276325
	);
	assert_eq!(
		PoolState::tick_at_sqrt_price(U256::from_dec_str("79228162514264337593543950").unwrap()),
		-138163
	);
	assert_eq!(
		PoolState::tick_at_sqrt_price(U256::from_dec_str("9903520314283042199192993792").unwrap()),
		-41591
	);
	assert_eq!(
		PoolState::tick_at_sqrt_price(U256::from_dec_str("28011385487393069959365969113").unwrap()),
		-20796
	);
	assert_eq!(
		PoolState::tick_at_sqrt_price(U256::from_dec_str("56022770974786139918731938227").unwrap()),
		-6932
	);
	assert_eq!(
		PoolState::tick_at_sqrt_price(U256::from_dec_str("79228162514264337593543950336").unwrap()),
		0
	);
	assert_eq!(
		PoolState::tick_at_sqrt_price(
			U256::from_dec_str("112045541949572279837463876454").unwrap()
		),
		6931
	);
	assert_eq!(
		PoolState::tick_at_sqrt_price(
			U256::from_dec_str("224091083899144559674927752909").unwrap()
		),
		20795
	);
	assert_eq!(
		PoolState::tick_at_sqrt_price(
			U256::from_dec_str("633825300114114700748351602688").unwrap()
		),
		41590
	);
	assert_eq!(
		PoolState::tick_at_sqrt_price(
			U256::from_dec_str("79228162514264337593543950336000").unwrap()
		),
		138162
	);
	assert_eq!(
		PoolState::tick_at_sqrt_price(
			U256::from_dec_str("79228162514264337593543950336000000").unwrap()
		),
		276324
	);
	assert_eq!(PoolState::tick_at_sqrt_price(MAX_SQRT_PRICE - 1), MAX_TICK - 1);
}

///////////////////////////////////////////////////////////
///               TEST SQRTPRICE MATH                  ////
///////////////////////////////////////////////////////////
#[test]
#[should_panic]
fn test_frominput_fails_zero() {
	// test Asset1ToAsset0 next_sqrt_price_from_input_amount
	Asset1ToAsset0::next_sqrt_price_from_input_amount(
		U256::from_dec_str("0").unwrap(),
		0,
		expandto18decimals(1) / 10,
	);
}
#[test]
#[should_panic]
fn test_frominput_fails_liqzero() {
	Asset0ToAsset1::next_sqrt_price_from_input_amount(
		U256::from_dec_str("1").unwrap(),
		0,
		expandto18decimals(1) / 10,
	);
}

// TODO: These should fail fix if we tighten up the data types
#[test]
//#[should_panic]
fn test_frominput_fails_inputoverflow() {
	Asset1ToAsset0::next_sqrt_price_from_input_amount(
		U256::from_dec_str("1461501637330902918203684832716283019655932542975").unwrap(), /* 2^160-1 */
		1024,
		U256::from_dec_str("1461501637330902918203684832716283019655932542976").unwrap(), /* 2^160 */
	);
}
#[test]
//#[should_panic]
fn test_frominput_fails_anyinputoverflow() {
	Asset1ToAsset0::next_sqrt_price_from_input_amount(
		U256::from_dec_str("1").unwrap(),
		1,
		U256::from_dec_str(
			"57896044618658097711785492504343953926634992332820282019728792003956564819968",
		)
		.unwrap(), //2^255
	);
}

#[test]
fn test_frominput_zeroamount_asset_0_to_asset_1() {
	let price = Asset0ToAsset1::next_sqrt_price_from_input_amount(
		encodedprice1_1(),
		expandto18decimals(1).as_u128(),
		U256::from_dec_str("0").unwrap(),
	);
	assert_eq!(price, encodedprice1_1());
}

#[test]
fn test_frominput_zeroamount_asset_1_to_asset_0() {
	let price = Asset1ToAsset0::next_sqrt_price_from_input_amount(
		encodedprice1_1(),
		expandto18decimals(1).as_u128(),
		U256::from_dec_str("0").unwrap(),
	);
	assert_eq!(price, encodedprice1_1());
}

#[test]
fn test_maxamounts_minprice() {
	let sqrt_p: U256 =
		U256::from_dec_str("1461501637330902918203684832716283019655932542976").unwrap();
	let liquidity: u128 = u128::MAX;
	let maxamount_nooverflow = U256::MAX - (liquidity << 96); // sqrt_p)

	let price = Asset0ToAsset1::next_sqrt_price_from_input_amount(
		sqrt_p, //2^96
		liquidity,
		maxamount_nooverflow,
	);

	assert_eq!(price, 1.into());
}

#[test]
fn test_frominput_inputamount_pair() {
	let price = Asset1ToAsset0::next_sqrt_price_from_input_amount(
		encodedprice1_1(), //encodePriceSqrt(1, 1)
		expandto18decimals(1).as_u128(),
		expandto18decimals(1) / 10,
	);
	assert_eq!(price, U256::from_dec_str("87150978765690771352898345369").unwrap());
}

#[test]
fn test_frominput_inputamount_base() {
	let price = Asset0ToAsset1::next_sqrt_price_from_input_amount(
		encodedprice1_1(), //encodePriceSqrt(1, 1)
		expandto18decimals(1).as_u128(),
		expandto18decimals(1) / 10,
	);
	assert_eq!(price, U256::from_dec_str("72025602285694852357767227579").unwrap());
}

#[test]
fn test_frominput_amountinmaxuint96_base() {
	let price = Asset0ToAsset1::next_sqrt_price_from_input_amount(
		encodedprice1_1(), //encodePriceSqrt(1, 1)
		expandto18decimals(10).as_u128(),
		U256::from_dec_str("1267650600228229401496703205376").unwrap(), // 2**100
	);
	assert_eq!(price, U256::from_dec_str("624999999995069620").unwrap());
}

#[test]
fn test_frominput_amountinmaxuint96_pair() {
	let price = Asset0ToAsset1::next_sqrt_price_from_input_amount(
		encodedprice1_1(), //encodePriceSqrt(1, 1)
		1u128,
		U256::MAX / 2,
	);
	assert_eq!(price, U256::from_dec_str("1").unwrap());
}

// Skip get amount from output

#[test]
fn test_expanded() {
	assert_eq!(expandto18decimals(1), expandto18decimals(1));
}

#[test]
fn test_0_if_liquidity_0() {
	assert_eq!(
		PoolState::asset_0_amount_delta_ceil(encodedprice1_1(), encodedprice2_1(), 0),
		U256::from(0)
	);
}

#[test]
fn test_price1_121() {
	assert_eq!(
		PoolState::asset_0_amount_delta_ceil(
			encodedprice1_1(),
			encodedprice121_100(),
			expandto18decimals(1).as_u128()
		),
		U256::from_dec_str("90909090909090910").unwrap()
	);

	assert_eq!(
		PoolState::asset_0_amount_delta_floor(
			encodedprice1_1(),
			encodedprice121_100(),
			expandto18decimals(1).as_u128()
		),
		U256::from_dec_str("90909090909090909").unwrap()
	);
}

#[test]
fn test_overflow() {
	assert_eq!(
		PoolState::asset_0_amount_delta_ceil(
			U256::from_dec_str("2787593149816327892691964784081045188247552").unwrap(),
			U256::from_dec_str("22300745198530623141535718272648361505980416").unwrap(),
			expandto18decimals(1).as_u128(),
		),
		PoolState::asset_0_amount_delta_floor(
			U256::from_dec_str("2787593149816327892691964784081045188247552").unwrap(),
			U256::from_dec_str("22300745198530623141535718272648361505980416").unwrap(),
			expandto18decimals(1).as_u128(),
		) + 1,
	);
}

// #getAmount1Delta

#[test]
fn test_0_if_liquidity_0_pair() {
	assert_eq!(
		PoolState::asset_1_amount_delta_ceil(encodedprice1_1(), encodedprice2_1(), 0),
		U256::from(0)
	);
}

#[test]
fn test_price1_121_pair() {
	assert_eq!(
		PoolState::asset_1_amount_delta_ceil(
			encodedprice1_1(),
			encodedprice121_100(),
			expandto18decimals(1).as_u128()
		),
		expandto18decimals(1) / 10
	);

	assert_eq!(
		PoolState::asset_1_amount_delta_floor(
			encodedprice1_1(),
			encodedprice121_100(),
			expandto18decimals(1).as_u128()
		),
		expandto18decimals(1) / 10 - 1
	);
}

// Swap computation
#[test]
fn test_sqrtoverflows() {
	let sqrt_p = U256::from_dec_str("1025574284609383690408304870162715216695788925244").unwrap();
	let liquidity = 50015962439936049619261659728067971248u128;
	let sqrt_q = Asset0ToAsset1::next_sqrt_price_from_input_amount(
		sqrt_p,
		liquidity,
		U256::from_dec_str("406").unwrap(),
	);
	assert_eq!(
		sqrt_q,
		U256::from_dec_str("1025574284609383582644711336373707553698163132913").unwrap()
	);

	assert_eq!(
		PoolState::asset_0_amount_delta_ceil(sqrt_q, sqrt_p, liquidity),
		U256::from_dec_str("406").unwrap()
	);
}

///////////////////////////////////////////////////////////
///                  TEST SWAPMATH                     ////
///////////////////////////////////////////////////////////

// computeSwapStep

// We cannot really fake the state of the pool to test this because we would need to mint a
// tick equivalent to desired sqrt_priceTarget but:
// sqrt_price_at_tick(tick_at_sqrt_price(sqrt_priceTarget)) != sqrt_priceTarget, due to the
// prices being between ticks - and therefore converting them to the closes tick.
#[test]
fn test_returns_error_asset_1_to_asset_0_fail() {
	let mut pool = PoolState::new(600, encodedprice1_1()).unwrap();
	let id: AccountId = AccountId::from([0xcf; 32]);

	let mut minted_capital = None;

	pool.mint(
		id,
		PoolState::tick_at_sqrt_price(encodedprice1_1()),
		PoolState::tick_at_sqrt_price(U256::from_dec_str("79623317895830914510487008059").unwrap()),
		expandto18decimals(2).as_u128(),
		|minted| {
			minted_capital.replace(minted);
			Ok::<(), ()>(())
		},
	)
	.unwrap();

	let _minted_capital = minted_capital.unwrap();
	// Swap to the right towards price target
	assert_eq!(
		pool.swap::<Asset1ToAsset0>(expandto18decimals(1)),
		Err(SwapError::InsufficientLiquidity)
	);
}

// Fake computeswapstep => Stripped down version of the real swap
// TODO: Consider refactoring real AMM to be able to easily test this.
// NOTE: Using ONE_IN_PIPS_UNISWAP here to match the tests. otherwise we would need decimals for
// the fee value
const ONE_IN_PIPS_UNISWAP: u32 = 1000000u32;

fn compute_swapstep<SD: SwapDirection>(
	current_sqrt_price: SqrtPriceQ64F96,
	sqrt_ratio_target: SqrtPriceQ64F96,
	liquidity: Liquidity,
	mut amount: AmountU256,
	fee: u32,
) -> (AmountU256, AmountU256, SqrtPriceQ64F96, U256) {
	let mut total_amount_out = AmountU256::zero();

	let amount_minus_fees = mul_div_floor(
		amount,
		U256::from(ONE_IN_PIPS_UNISWAP - fee),
		U256::from(ONE_IN_PIPS_UNISWAP),
	); // This cannot overflow as we bound fee_pips to <= ONE_IN_HUNDREDTH_BIPS/2 (TODO)

	let amount_required_to_reach_target =
		SD::input_amount_delta_ceil(current_sqrt_price, sqrt_ratio_target, liquidity);

	let sqrt_ratio_next = if amount_minus_fees >= amount_required_to_reach_target {
		sqrt_ratio_target
	} else {
		assert!(liquidity != 0);
		SD::next_sqrt_price_from_input_amount(current_sqrt_price, liquidity, amount_minus_fees)
	};

	// Cannot overflow as if the swap traversed all ticks (MIN_TICK to MAX_TICK
	// (inclusive)), assuming the maximum possible liquidity, total_amount_out would still
	// be below U256::MAX (See test `output_amounts_bounded`)
	total_amount_out +=
		SD::output_amount_delta_floor(current_sqrt_price, sqrt_ratio_next, liquidity);

	// next_sqrt_price_from_input_amount rounds so this maybe Ok(()) even though
	// amount_minus_fees < amount_required_to_reach_target (TODO Prove)
	if sqrt_ratio_next == sqrt_ratio_target {
		// Will not overflow as fee_pips <= ONE_IN_HUNDREDTH_BIPS / 2
		let fees = mul_div_ceil(
			amount_required_to_reach_target,
			U256::from(fee),
			U256::from(ONE_IN_PIPS_UNISWAP - fee),
		);

		// TODO: Prove these don't underflow
		amount -= amount_required_to_reach_target;
		amount -= fees;
		(amount_required_to_reach_target, total_amount_out, sqrt_ratio_next, fees)
	} else {
		let amount_in = SD::input_amount_delta_ceil(current_sqrt_price, sqrt_ratio_next, liquidity);
		// Will not underflow due to rounding in flavor of the pool of both sqrt_ratio_next
		// and amount_in. (TODO: Prove)
		let fees = amount - amount_in;
		(amount_in, total_amount_out, sqrt_ratio_next, fees)
	}
}

#[test]
fn test_amount_capped_asset_1_to_asset_0() {
	let price = encodedprice1_1();
	let amount = expandto18decimals(1);
	let price_target = U256::from_dec_str("79623317895830914510487008059").unwrap();
	let liquidity = expandto18decimals(2).as_u128();
	let (amount_in, amount_out, sqrt_ratio_next, fees) =
		compute_swapstep::<Asset1ToAsset0>(price, price_target, liquidity, amount, 600);

	assert_eq!(amount_in, U256::from_dec_str("9975124224178055").unwrap());
	assert_eq!(fees, U256::from_dec_str("5988667735148").unwrap());
	assert_eq!(amount_out, U256::from_dec_str("9925619580021728").unwrap());
	assert!(amount_in + fees < amount);

	let price_after_input_amount =
		PoolState::next_sqrt_price_from_asset_1_input(price, liquidity, amount);

	assert_eq!(sqrt_ratio_next, price_target);
	assert!(sqrt_ratio_next < price_after_input_amount);
}

// Skip amountout test

#[test]
fn test_amount_in_spent_asset_1_to_asset_0() {
	let price = encodedprice1_1();
	let price_target = U256::from_dec_str("792281625142643375935439503360").unwrap();
	let liquidity = expandto18decimals(2).as_u128();
	let amount = expandto18decimals(1);
	let (amount_in, amount_out, sqrt_ratio_next, fees) =
		compute_swapstep::<Asset1ToAsset0>(price, price_target, liquidity, amount, 600);

	assert_eq!(amount_in, U256::from_dec_str("999400000000000000").unwrap());
	assert_eq!(fees, U256::from_dec_str("600000000000000").unwrap());
	assert_eq!(amount_out, U256::from_dec_str("666399946655997866").unwrap());
	assert_eq!(amount_in + fees, amount);

	let price_after_input_amount =
		PoolState::next_sqrt_price_from_asset_1_input(price, liquidity, amount - fees);

	assert!(sqrt_ratio_next < price_target);
	assert_eq!(sqrt_ratio_next, price_after_input_amount);
}

#[test]
fn test_target_price1_partial_input() {
	let (amount_in, amount_out, sqrt_ratio_next, fees) = compute_swapstep::<Asset0ToAsset1>(
		U256::from_dec_str("2").unwrap(),
		U256::from_dec_str("1").unwrap(),
		1u128,
		U256::from_dec_str("3915081100057732413702495386755767").unwrap(),
		1,
	);
	assert_eq!(amount_in, U256::from_dec_str("39614081257132168796771975168").unwrap());
	assert_eq!(fees, U256::from_dec_str("39614120871253040049813").unwrap());
	assert!(amount_in + fees < U256::from_dec_str("3915081100057732413702495386755767").unwrap());
	assert_eq!(amount_out, U256::from(0));
	assert_eq!(sqrt_ratio_next, U256::from_dec_str("1").unwrap());
}

#[test]
fn test_entireinput_asfee() {
	let (amount_in, amount_out, sqrt_ratio_next, fees) = compute_swapstep::<Asset1ToAsset0>(
		U256::from_dec_str("2413").unwrap(),
		U256::from_dec_str("79887613182836312").unwrap(),
		1985041575832132834610021537970u128,
		U256::from_dec_str("10").unwrap(),
		1872,
	);
	assert_eq!(amount_in, U256::from_dec_str("0").unwrap());
	assert_eq!(fees, U256::from_dec_str("10").unwrap());
	assert_eq!(amount_out, U256::from_dec_str("0").unwrap());
	assert_eq!(sqrt_ratio_next, U256::from_dec_str("2413").unwrap());
}
