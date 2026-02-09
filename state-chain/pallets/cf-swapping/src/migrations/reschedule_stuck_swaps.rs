use core::marker::PhantomData;

use cf_primitives::{Asset, AssetAmount, SwapId, SwapRequestId, SWAP_DELAY_BLOCKS};
use frame_support::{
	traits::{Get, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use sp_runtime::traits::BlockNumberProvider;

use crate::{
	Config, Event, ScheduledSwaps, Swap, SwapFailureReason, SwapRequestState, SwapRequests,
};

pub struct RescheduleStuckSwaps<T>(PhantomData<T>);

use sp_std::{vec, vec::Vec};

impl<T: Config> UncheckedOnRuntimeUpgrade for RescheduleStuckSwaps<T> {
	fn on_runtime_upgrade() -> Weight {
		fn swap<T: Config>(
			swap_request_id: SwapRequestId,
			swap_id: SwapId,
			from: Asset,
			to: Asset,
			input_amount: AssetAmount,
		) -> Swap<T> {
			let execute_at =
				frame_system::Pallet::<T>::current_block_number() + SWAP_DELAY_BLOCKS.into();

			Swap {
				swap_id,
				swap_request_id,
				from,
				to,
				input_amount,
				refund_params: None,
				execute_at,
			}
		}

		let swaps_to_reschedule: Vec<Swap<T>> = vec![
			swap(SwapRequestId(896044), SwapId(1209544), Asset::Usdt, Asset::Eth, 1000691), // ~$1
			swap(SwapRequestId(892485), SwapId(1205341), Asset::Btc, Asset::Flip, 1312),    /* ~$1.6 */
			swap(SwapRequestId(896909), SwapId(1210549), Asset::Usdt, Asset::Eth, 1709416), /* ~$1.7 */
			swap(SwapRequestId(896575), SwapId(1210180), Asset::Usdt, Asset::Eth, 1986577), // ~$2
			swap(SwapRequestId(949651), SwapId(1275236), Asset::Eth, Asset::Flip, 127252664848955), /* ~$0.5 */
			swap(SwapRequestId(879667), SwapId(1189291), Asset::Usdt, Asset::Eth, 394506), /* ~$0.39 */
			swap(SwapRequestId(879641), SwapId(1189264), Asset::Usdt, Asset::Eth, 392535), /* ~$0.39 */
			swap(SwapRequestId(896481), SwapId(1210072), Asset::Usdt, Asset::Eth, 2581412), /* ~$2.58 */
			swap(SwapRequestId(1079344), SwapId(1434844), Asset::HubUsdc, Asset::HubDot, 78920), /* ~$0.08 */
			swap(SwapRequestId(1079442), SwapId(1434954), Asset::ArbUsdc, Asset::ArbEth, 100110), // ~$0.1
			swap(SwapRequestId(896438), SwapId(1210027), Asset::Usdt, Asset::Eth, 2068310), /* ~$2.06 */
			swap(SwapRequestId(896845), SwapId(1210466), Asset::Usdt, Asset::Eth, 1209801), /* ~$1.21 */
			swap(SwapRequestId(939503), SwapId(1262026), Asset::ArbUsdc, Asset::ArbEth, 100100), // ~$0.1
			swap(SwapRequestId(897400), SwapId(1211156), Asset::Usdt, Asset::Eth, 545669), /* ~$0.55 */
			swap(SwapRequestId(896437), SwapId(1210026), Asset::Usdt, Asset::Eth, 2068310), /* ~$2.07 */
			swap(SwapRequestId(896976), SwapId(1210629), Asset::Usdt, Asset::Eth, 1362157), /* ~$1.36 */
			swap(SwapRequestId(974972), SwapId(1306997), Asset::Usdt, Asset::Eth, 4453645), /* ~$4.45 */
			swap(SwapRequestId(949649), SwapId(1275234), Asset::Eth, Asset::Flip, 127252664848955), // ~$0.5
			swap(SwapRequestId(895463), SwapId(1208649), Asset::Usdc, Asset::Eth, 888268), /* ~$0.89 */
			swap(SwapRequestId(896545), SwapId(1210150), Asset::Usdt, Asset::Eth, 3039256), // ~$3
			swap(SwapRequestId(896501), SwapId(1210105), Asset::Usdt, Asset::Eth, 2166963), /* ~$2.17 */
			swap(SwapRequestId(1111546), SwapId(1472445), Asset::Usdc, Asset::Eth, 466396), /* ~$0.47 */
			swap(SwapRequestId(880176), SwapId(1190054), Asset::Usdt, Asset::Eth, 512360), /* ~$0.51 */
			swap(SwapRequestId(896427), SwapId(1210016), Asset::Usdt, Asset::Eth, 1061174), /* ~$1.06 */
			swap(SwapRequestId(1079424), SwapId(1434933), Asset::ArbUsdc, Asset::ArbEth, 116561), /* ~$0.12 */
			swap(SwapRequestId(897073), SwapId(1210727), Asset::Usdt, Asset::Eth, 700399),        /* ~$0.7 */
			swap(SwapRequestId(896398), SwapId(1209984), Asset::Usdt, Asset::Eth, 1176628), /* ~$1.18 */
			swap(SwapRequestId(949648), SwapId(1275233), Asset::Eth, Asset::Flip, 127252664848955), /* ~$0.5 */
			swap(SwapRequestId(896399), SwapId(1209985), Asset::Usdt, Asset::Eth, 1176628), /* ~$1.18 */
			swap(SwapRequestId(896893), SwapId(1210530), Asset::Usdt, Asset::Eth, 1711040), /* ~$1.71 */
			swap(SwapRequestId(896786), SwapId(1210407), Asset::Usdt, Asset::Eth, 1425100), /* ~$1.43 */
			swap(SwapRequestId(1079447), SwapId(1434959), Asset::ArbUsdc, Asset::ArbEth, 90099), /* ~$0.09 */
			swap(SwapRequestId(896036), SwapId(1209536), Asset::Usdt, Asset::Eth, 1088559), /* ~$1.09 */
			swap(SwapRequestId(896401), SwapId(1209987), Asset::Usdt, Asset::Eth, 1162947), /* ~$1.16 */
			swap(SwapRequestId(896864), SwapId(1210485), Asset::Usdt, Asset::Eth, 1472520), /* ~$1.47 */
			swap(SwapRequestId(896975), SwapId(1210628), Asset::Usdt, Asset::Eth, 1654415), /* ~$1.66 */
			swap(SwapRequestId(896574), SwapId(1210179), Asset::Usdt, Asset::Eth, 1986577), /* ~$1.99 */
			swap(SwapRequestId(896944), SwapId(1210595), Asset::Usdt, Asset::Eth, 1848305), /* ~$1.85 */
			swap(SwapRequestId(896967), SwapId(1210620), Asset::Usdt, Asset::Eth, 1294105), /* ~$1.29 */
			swap(SwapRequestId(897508), SwapId(1211291), Asset::Usdc, Asset::Eth, 481255),  /* ~$0.48 */
			swap(SwapRequestId(883276), SwapId(1193813), Asset::Usdt, Asset::Eth, 501581),  /* ~$0.5 */
			swap(SwapRequestId(1028183), SwapId(1374070), Asset::Usdt, Asset::Eth, 2485389), /* ~$2.48 */
			swap(SwapRequestId(1079441), SwapId(1434953), Asset::ArbUsdc, Asset::ArbEth, 100110), /* ~$0.1 */
			swap(SwapRequestId(896089), SwapId(1209594), Asset::Usdt, Asset::Eth, 1084716), /* ~$1.09 */
			swap(SwapRequestId(1218271), SwapId(1594698), Asset::Usdt, Asset::Eth, 11948),  /* ~$0.01 */
			swap(SwapRequestId(879681), SwapId(1189305), Asset::Usdt, Asset::Eth, 392906),  /* ~$0.39 */
			swap(SwapRequestId(880175), SwapId(1190053), Asset::Usdt, Asset::Eth, 512360),  /* ~$0.51 */
			swap(SwapRequestId(974956), SwapId(1306969), Asset::Usdt, Asset::Eth, 5279226), /* ~$5.28 */
			swap(SwapRequestId(896870), SwapId(1210492), Asset::Usdt, Asset::Eth, 1414035), /* ~$1.41 */
			swap(SwapRequestId(890922), SwapId(1203622), Asset::Btc, Asset::Flip, 400),     /* ~$0.5 */
			swap(SwapRequestId(896030), SwapId(1209530), Asset::Usdt, Asset::Eth, 1088559), /* ~$1.09 */
			swap(SwapRequestId(879774), SwapId(1189435), Asset::Usdt, Asset::Eth, 429964),  /* ~$0.43 */
			swap(SwapRequestId(896931), SwapId(1210579), Asset::Usdt, Asset::Eth, 1674208), /* ~$1.68 */
			swap(SwapRequestId(896465), SwapId(1210055), Asset::Usdt, Asset::Eth, 2568292), /* ~$2.56 */
			swap(SwapRequestId(911947), SwapId(1228868), Asset::Usdc, Asset::Flip, 19634462), /* ~$19.63 */
			swap(SwapRequestId(896914), SwapId(1210555), Asset::Usdt, Asset::Eth, 1709416), /* ~$1.71 */
			swap(SwapRequestId(930185), SwapId(1250310), Asset::Usdc, Asset::Eth, 1017796), /* ~$1 */
			swap(SwapRequestId(896445), SwapId(1210035), Asset::Usdt, Asset::Eth, 2242742), /* ~$2.24 */
			swap(SwapRequestId(939466), SwapId(1261989), Asset::ArbUsdc, Asset::ArbEth, 147477), /* ~$0.15 */
			swap(SwapRequestId(880805), SwapId(1190755), Asset::Usdt, Asset::Eth, 1009198), /* ~$1 */
			swap(SwapRequestId(896032), SwapId(1209532), Asset::Usdt, Asset::Eth, 1088559), /* ~$1 */
			swap(SwapRequestId(896033), SwapId(1209533), Asset::Usdt, Asset::Eth, 1088559), /* ~$1.09 */
			swap(SwapRequestId(897402), SwapId(1211158), Asset::Usdt, Asset::Eth, 545669),  /* ~$0.55 */
			swap(SwapRequestId(879666), SwapId(1189290), Asset::Usdt, Asset::Eth, 394506),  /* ~$0.39 */
			swap(SwapRequestId(949534), SwapId(1275032), Asset::Usdt, Asset::Eth, 42780),   /* ~$0.04 */
			swap(SwapRequestId(897403), SwapId(1211159), Asset::Usdt, Asset::Eth, 545669),  /* ~$0.55 */
			swap(SwapRequestId(896573), SwapId(1210178), Asset::Usdc, Asset::Eth, 1987461), /* ~$2 */
			swap(SwapRequestId(1079456), SwapId(1434968), Asset::ArbUsdc, Asset::ArbEth, 189370), /* ~$0.19 */
			swap(SwapRequestId(895792), SwapId(1434968), Asset::Usdt, Asset::Eth, 189370), /* ~$0.19 */
			swap(SwapRequestId(879853), SwapId(1189586), Asset::Usdt, Asset::Eth, 546510), /* ~$0.55 */
			swap(SwapRequestId(896913), SwapId(1210554), Asset::Usdt, Asset::Eth, 1709416), /* ~$1.71 */
			swap(SwapRequestId(1079455), SwapId(1434967), Asset::ArbUsdc, Asset::ArbEth, 150886), /* ~$0.15 */
			swap(SwapRequestId(949536), SwapId(1275034), Asset::Usdc, Asset::Eth, 42793), /* ~$0.04 */
			swap(SwapRequestId(897401), SwapId(1211157), Asset::Usdt, Asset::Eth, 545669), /* ~$0.55 */
			swap(SwapRequestId(949650), SwapId(1275235), Asset::Eth, Asset::Flip, 127252664848955), /* ~$0.5 */
			swap(SwapRequestId(896946), SwapId(1210597), Asset::Usdt, Asset::Eth, 1678763), /* ~$1.68 */
			swap(SwapRequestId(896787), SwapId(1210408), Asset::Usdt, Asset::Eth, 1425100), /* ~$1.43 */
			//
			swap(SwapRequestId(1079450), SwapId(1434962), Asset::ArbUsdc, Asset::ArbEth, 90099), /* ~$0.09 */
			swap(SwapRequestId(880043), SwapId(1189848), Asset::Usdt, Asset::Eth, 292921),       /* ~$0.29 */
			swap(SwapRequestId(896949), SwapId(1210602), Asset::Usdt, Asset::Eth, 1850984),      /* ~$1.85 */
			swap(
				SwapRequestId(975047),
				SwapId(1307105),
				Asset::Flip,
				Asset::Eth,
				9306682671012647554,
			), /* ~$3.10 */
			swap(SwapRequestId(879854), SwapId(1189587), Asset::Usdt, Asset::Eth, 546510),       /* ~$0.55 */
			swap(SwapRequestId(879388), SwapId(1188900), Asset::Usdt, Asset::Eth, 439034),       /* ~$0.44 */
			swap(SwapRequestId(896910), SwapId(1210551), Asset::Usdt, Asset::Eth, 1709416),      /* ~$1.71 */
			swap(SwapRequestId(896420), SwapId(1210007), Asset::Usdc, Asset::Eth, 1002871),      /* ~$1 */
			swap(SwapRequestId(896844), SwapId(1210465), Asset::Usdt, Asset::Eth, 1209801),      /* ~$1.21 */
			swap(SwapRequestId(876543), SwapId(1185061), Asset::Usdt, Asset::Eth, 254048),       /* ~$0.25 */
			swap(SwapRequestId(939495), SwapId(1262018), Asset::ArbUsdc, Asset::ArbEth, 100100), /* ~$0.1 */
			swap(SwapRequestId(896846), SwapId(1210467), Asset::Usdt, Asset::Eth, 1209801),      /* ~$1.21 */
			swap(SwapRequestId(896499), SwapId(1210103), Asset::Usdt, Asset::Eth, 2680304),      /* ~$2.68 */
			swap(SwapRequestId(1079452), SwapId(1434964), Asset::ArbUsdc, Asset::ArbEth, 100110), /* ~$0.1 */
			swap(SwapRequestId(880174), SwapId(1190052), Asset::Usdt, Asset::Eth, 512360),       /* ~$0.51 */
			swap(SwapRequestId(1028162), SwapId(1374049), Asset::Usdt, Asset::Eth, 1535518),     /* ~$1.53 */
			swap(SwapRequestId(1028180), SwapId(1374067), Asset::Usdt, Asset::Eth, 2055369),     /* ~$2.05 */
			swap(SwapRequestId(1079435), SwapId(1434947), Asset::ArbUsdc, Asset::ArbEth, 100110), /* ~$0.1 */
			swap(SwapRequestId(895791), SwapId(1209167), Asset::Usdt, Asset::Eth, 528971),       /* ~$0.53 */
			swap(SwapRequestId(896865), SwapId(1210486), Asset::Usdt, Asset::Eth, 1472520),      /* ~$1.47 */
			swap(SwapRequestId(899512), SwapId(1213456), Asset::Btc, Asset::Flip, 403),          /* ~$0.5 */
			swap(SwapRequestId(1079427), SwapId(1434937), Asset::ArbUsdc, Asset::ArbEth, 104695), /* ~$0.1 */
			swap(SwapRequestId(1118520), SwapId(1480596), Asset::ArbUsdc, Asset::ArbEth, 100100), /* ~$0.1 */
			swap(
				SwapRequestId(1028087),
				SwapId(1373925),
				Asset::Flip,
				Asset::Eth,
				2235699640687331880,
			), /* ~$0.9 */
			swap(SwapRequestId(896912), SwapId(1210553), Asset::Usdt, Asset::Eth, 1709416),      /* ~$1.71 */
			swap(
				SwapRequestId(896933),
				SwapId(1210582),
				Asset::Flip,
				Asset::Eth,
				2236378835677996574,
			), /* ~$1.3 */
			swap(SwapRequestId(879399), SwapId(1188911), Asset::Usdt, Asset::Eth, 492974),       /* ~$0.49 */
			swap(SwapRequestId(896550), SwapId(1210155), Asset::Usdt, Asset::Eth, 2359693),      /* ~$2.36 */
			swap(SwapRequestId(892484), SwapId(1205340), Asset::Btc, Asset::Flip, 212),          /* ~$0.26 */
			swap(SwapRequestId(896035), SwapId(1209535), Asset::Usdt, Asset::Eth, 1088559),      /* ~$1.09 */
			swap(SwapRequestId(883237), SwapId(1193773), Asset::Usdt, Asset::Eth, 376962),       /* ~$0.38 */
			swap(SwapRequestId(896423), SwapId(1210012), Asset::Usdc, Asset::Eth, 1047049),      /* ~$1.05 */
			swap(SwapRequestId(880829), SwapId(1190779), Asset::Usdt, Asset::Eth, 1500549),      /* ~$1.50 */
			swap(SwapRequestId(949535), SwapId(1275033), Asset::Usdc, Asset::Eth, 42793),        /* ~$0.04 */
			swap(SwapRequestId(883277), SwapId(1193814), Asset::Usdt, Asset::Eth, 501581),       /* ~$0.5 */
			swap(SwapRequestId(896848), SwapId(1210469), Asset::Usdt, Asset::Eth, 1209801),      /* ~$1.21 */
			swap(SwapRequestId(912313), SwapId(1229467), Asset::Usdt, Asset::Eth, 20106361),     /* ~$20.1 */
			swap(SwapRequestId(896977), SwapId(1210630), Asset::Usdt, Asset::Eth, 1502804),      /* ~$1.50 */
			swap(SwapRequestId(912088), SwapId(1229111), Asset::ArbUsdc, Asset::ArbEth, 55333441), /* ~$55.33 */
			swap(SwapRequestId(895803), SwapId(1209182), Asset::Usdt, Asset::Eth, 561453), /* ~$0.56 */
			swap(SwapRequestId(1028166), SwapId(1374053), Asset::Usdt, Asset::Eth, 1372825), /* ~$1.37 */
			swap(SwapRequestId(896034), SwapId(1209534), Asset::Usdt, Asset::Eth, 1088559), /* ~$1.09 */
			swap(SwapRequestId(896990), SwapId(1210643), Asset::Usdt, Asset::Eth, 2710653), /* ~$2.71 */
			swap(SwapRequestId(896015), SwapId(1209512), Asset::Usdc, Asset::Eth, 1017440), /* ~$1.02 */
			swap(SwapRequestId(896791), SwapId(1210412), Asset::Usdt, Asset::Eth, 1117210), /* ~$1.12 */
			swap(SwapRequestId(879640), SwapId(1189263), Asset::Usdt, Asset::Eth, 392535), /* ~$0.39 */
			swap(SwapRequestId(895790), SwapId(1209166), Asset::Usdt, Asset::Eth, 528971), /* ~$0.53 */
			swap(SwapRequestId(1079454), SwapId(1434966), Asset::ArbUsdc, Asset::ArbEth, 120568), /* ~$0.12 */
			swap(SwapRequestId(915798), SwapId(1233615), Asset::ArbUsdc, Asset::ArbEth, 90090), /* ~$0.09 */
			swap(SwapRequestId(896847), SwapId(1210468), Asset::Usdt, Asset::Eth, 1209801),     /* ~$1.21 */
			swap(SwapRequestId(886400), SwapId(1197786), Asset::Btc, Asset::Flip, 12500),       /* ~$15.34 */
			swap(SwapRequestId(903602), SwapId(1218425), Asset::Sol, Asset::Flip, 2259124),     /* ~$0.5 */
		];

		let mut rescheduled_swaps_count = 0;

		ScheduledSwaps::<T>::mutate(|scheduled_swaps| {
			for swap in &swaps_to_reschedule {
				// Sanity check: only reschedule if swap request does exist and assets match
				if let Some(swap_request) = SwapRequests::<T>::get(swap.swap_request_id) {
					if swap_request.input_asset == swap.from &&
						swap_request.output_asset == swap.to &&
						// Only fee swaps are expected:
						(swap_request.state == SwapRequestState::IngressEgressFee ||
							swap_request.state == SwapRequestState::NetworkFee)
					{
						crate::Pallet::<T>::deposit_event(Event::<T>::SwapRescheduled {
							swap_id: swap.swap_id,
							execute_at: swap.execute_at,
							reason: SwapFailureReason::PriceImpactLimit,
						});

						log::info!("Rescheduled swap: {}", swap.swap_request_id);

						scheduled_swaps.insert(swap.swap_id, swap.clone());

						rescheduled_swaps_count += 1;
					} else {
						log::error!(
							"Parameters don't match for swap request: {}",
							swap.swap_request_id
						);
					}
				} else {
					log::warn!("Swap request does not exist: {}", swap.swap_request_id);
				}
			}
		});

		T::DbWeight::get()
			.reads_writes(1 + swaps_to_reschedule.len() as u64, 1 + rescheduled_swaps_count as u64)
	}
}
