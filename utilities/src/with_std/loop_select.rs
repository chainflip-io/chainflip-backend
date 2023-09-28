#[doc(hidden)]
pub use futures::future::ready as internal_ready;
#[doc(hidden)]
pub use tokio::select as internal_tokio_select;

#[macro_export]
macro_rules! inner_loop_select {
    ($disabled_mask:ident, $count:expr, { $($processed:tt)* } let $pattern:pat = $expression:expr => $body:block, $($unprocessed:tt)*) => {
        $crate::inner_loop_select!(
			$disabled_mask,
			$count + 1u64,
            {
                $($processed)*
                x = $expression => {
					let $pattern = x;
					$body
				},
            }
            $($unprocessed)*
		)
    };
    ($disabled_mask:ident, $count:expr, { $($processed:tt)* } if let $pattern:pat = $expression:expr => $body:block, $($unprocessed:tt)*) => {
        $crate::inner_loop_select!(
			$disabled_mask,
			$count + 1u64,
            {
                $($processed)*
                x = $expression => {
					if let $pattern = x {
						$body
					} else { break }
				},
            }
            $($unprocessed)*
		)
    };
    ($disabled_mask:ident, $count:expr, { $($processed:tt)* } if let $pattern:pat = $expression:expr => $body:block else break $extra:expr, $($unprocessed:tt)*) => {
        $crate::inner_loop_select!(
			$disabled_mask,
			$count + 1u64,
            {
                $($processed)*
                x = $expression => {
					if let $pattern = x {
						$body
					} else { break $extra }
				},
            }
            $($unprocessed)*
		)
    };
	($disabled_mask:ident, $count:expr, { $($processed:tt)* } if let $pattern:pat = $expression:expr => $body:block else disable $(then if $disable_break_expression:expr => break $($extra:expr)?)?, $($unprocessed:tt)*) => {
        $crate::inner_loop_select!(
			$disabled_mask,
			$count + 1u64,
            {
                $($processed)*
                x = async { $expression.await }, if $disabled_mask & (1u64 << $count) != (1u64 << $count) => {
					if let $pattern = x {
						$body
					} else {
						$disabled_mask |= 1u64 << $count;
					}
				},
				$(
					_ = $crate::loop_select::internal_ready(()), if $disabled_mask & (1u64 << $count) == (1u64 << $count) && $disable_break_expression => {
						break $($extra)?
					},
				)?
            }
            $($unprocessed)*
		)
    };
	($disabled_mask:ident, $count:expr, { $($processed:tt)* } if $enable_expression:expr => let $pattern:pat = $expression:expr => $body:block, $($unprocessed:tt)*) => {
        $crate::inner_loop_select!(
			$disabled_mask,
			$count + 1u64,
            {
                $($processed)*
                x = async { $expression.await }, if $enable_expression => {
					let $pattern = x;
					$body
				},
            }
            $($unprocessed)*
		)
    };
	($disabled_mask:ident, $count:expr, { $($processed:tt)* } if $enable_expression:expr => if let $pattern:pat = $expression:expr => $body:block, $($unprocessed:tt)*) => {
        $crate::inner_loop_select!(
			$disabled_mask,
			$count + 1u64,
            {
                $($processed)*
                x = async { $expression.await }, if $enable_expression => {
					if let $pattern = x {
						$body
					} else { break }
				},
            }
            $($unprocessed)*
		)
    };
	($disabled_mask:ident, $count:expr, { $($processed:tt)* } if $enable_expression:expr => if let $pattern:pat = $expression:expr => $body:block else break $extra:expr, $($unprocessed:tt)*) => {
        $crate::inner_loop_select!(
			$disabled_mask,
			$count + 1u64,
            {
                $($processed)*
                x = async { $expression.await }, if $enable_expression => {
					if let $pattern = x {
						$body
					} else { break $extra }
				},
            }
            $($unprocessed)*
		)
    };
	($disabled_mask:ident, $count:expr, { $($processed:tt)* } if $expression:expr => break $($extra:expr)?, $($unprocessed:tt)*) => {
        $crate::inner_loop_select!(
			$disabled_mask,
			$count + 1u64,
            {
                $($processed)*
                _ = $crate::loop_select::internal_ready(()), if $expression => {
					break $($extra)?
				},
            }
            $($unprocessed)*
		)
    };
    ($disabled_mask:ident, $count:expr, { $($processed:tt)+ }) => {
		loop {
			$crate::loop_select::internal_tokio_select!(
				$($processed)+
			)
		}
    };
}

#[macro_export]
macro_rules! loop_select {
    ($($cases:tt)+) => {{
		#[allow(unused, unused_mut)]
		let mut disabled_mask = 0u64;
        $crate::inner_loop_select!(disabled_mask, 0u64, {} $($cases)+)
    }}
}

#[cfg(test)]
mod test_loop_select {
	use futures::StreamExt;

	#[allow(clippy::unit_cmp)]
	#[tokio::test]
	async fn exits_loop_on_branch_failure() {
		const BREAK_VALUE: u32 = 1;

		// Single branch

		assert_eq!(
			(),
			loop_select!(
				if let 'a' = futures::future::ready('b') => { panic!() },
			)
		);
		assert_eq!(
			BREAK_VALUE,
			loop_select!(
				if let 'a' = futures::future::ready('b') => { panic!() } else break BREAK_VALUE,
			)
		);
		assert_eq!(
			(),
			loop_select!(
				if true => if let 'a' = futures::future::ready('b') => { panic!() },
			)
		);
		assert_eq!(
			BREAK_VALUE,
			loop_select!(
				if true => if let 'a' = futures::future::ready('b') => { panic!() } else break BREAK_VALUE,
			)
		);

		// Multiple branches

		// Other branch is Ready
		{
			// Other branch has no enable expression
			assert_eq!(
				(),
				loop_select!(
					if let 'a' = futures::future::ready('b') => { panic!() },
					let _ = futures::future::ready('c') => {},
				)
			);
			assert_eq!(
				BREAK_VALUE,
				loop_select!(
					if let 'a' = futures::future::ready('b') => { panic!() } else break BREAK_VALUE,
					let _ = futures::future::ready('c') => {},
				)
			);
			assert_eq!(
				(),
				loop_select!(
					if true => if let 'a' = futures::future::ready('b') => { panic!() },
					let _ = futures::future::ready('c') => {},
				)
			);
			assert_eq!(
				BREAK_VALUE,
				loop_select!(
					if true => if let 'a' = futures::future::ready('b') => { panic!() } else break BREAK_VALUE,
					let _ = futures::future::ready('c') => {},
				)
			);

			// Other branch is disabled
			assert_eq!(
				(),
				loop_select!(
					if let 'a' = futures::future::ready('b') => { panic!() },
					if false => let _ = futures::future::ready('c') => {},
				)
			);
			assert_eq!(
				BREAK_VALUE,
				loop_select!(
					if let 'a' = futures::future::ready('b') => { panic!() } else break BREAK_VALUE,
					if false => let _ = futures::future::ready('c') => {},
				)
			);
			assert_eq!(
				(),
				loop_select!(
					if true => if let 'a' = futures::future::ready('b') => { panic!() },
					if false => let _ = futures::future::ready('c') => {},
				)
			);
			assert_eq!(
				BREAK_VALUE,
				loop_select!(
					if true => if let 'a' = futures::future::ready('b') => { panic!() } else break BREAK_VALUE,
					if false => let _ = futures::future::ready('c') => {},
				)
			);

			// Other branch is enabled
			assert_eq!(
				(),
				loop_select!(
					if let 'a' = futures::future::ready('b') => { panic!() },
					if true => let _ = futures::future::ready('c') => {},
				)
			);
			assert_eq!(
				BREAK_VALUE,
				loop_select!(
					if let 'a' = futures::future::ready('b') => { panic!() } else break BREAK_VALUE,
					if true => let _ = futures::future::ready('c') => {},
				)
			);
			assert_eq!(
				(),
				loop_select!(
					if true => if let 'a' = futures::future::ready('b') => { panic!() },
					if true => let _ = futures::future::ready('c') => {},
				)
			);
			assert_eq!(
				BREAK_VALUE,
				loop_select!(
					if true => if let 'a' = futures::future::ready('b') => { panic!() } else break BREAK_VALUE,
					if true => let _ = futures::future::ready('c') => {},
				)
			);
		}

		// Other branch is Pending
		{
			// Other branch has no enable expression
			assert_eq!(
				(),
				loop_select!(
					if let 'a' = futures::future::ready('b') => { panic!() },
					let _ = futures::future::pending::<u32>() => {},
				)
			);
			assert_eq!(
				BREAK_VALUE,
				loop_select!(
					if let 'a' = futures::future::ready('b') => { panic!() } else break BREAK_VALUE,
					let _ = futures::future::pending::<u32>() => {},
				)
			);
			assert_eq!(
				(),
				loop_select!(
					if true => if let 'a' = futures::future::ready('b') => { panic!() },
					let _ = futures::future::pending::<u32>() => {},
				)
			);
			assert_eq!(
				BREAK_VALUE,
				loop_select!(
					if true => if let 'a' = futures::future::ready('b') => { panic!() } else break BREAK_VALUE,
					let _ = futures::future::pending::<u32>() => {},
				)
			);

			// Other branch is disabled
			assert_eq!(
				(),
				loop_select!(
					if let 'a' = futures::future::ready('b') => { panic!() },
					if false => let _ = futures::future::pending::<u32>() => {},
				)
			);
			assert_eq!(
				BREAK_VALUE,
				loop_select!(
					if let 'a' = futures::future::ready('b') => { panic!() } else break BREAK_VALUE,
					if false => let _ = futures::future::pending::<u32>() => {},
				)
			);
			assert_eq!(
				(),
				loop_select!(
					if true => if let 'a' = futures::future::ready('b') => { panic!() },
					if false => let _ = futures::future::pending::<u32>() => {},
				)
			);
			assert_eq!(
				BREAK_VALUE,
				loop_select!(
					if true => if let 'a' = futures::future::ready('b') => { panic!() } else break BREAK_VALUE,
					if false => let _ = futures::future::pending::<u32>() => {},
				)
			);

			// Other branch is enabled
			assert_eq!(
				(),
				loop_select!(
					if let 'a' = futures::future::ready('b') => { panic!() },
					if true => let _ = futures::future::pending::<u32>() => {},
				)
			);
			assert_eq!(
				BREAK_VALUE,
				loop_select!(
					if let 'a' = futures::future::ready('b') => { panic!() } else break BREAK_VALUE,
					if true => let _ = futures::future::pending::<u32>() => {},
				)
			);
			assert_eq!(
				(),
				loop_select!(
					if true => if let 'a' = futures::future::ready('b') => { panic!() },
					if true => let _ = futures::future::pending::<u32>() => {},
				)
			);
			assert_eq!(
				BREAK_VALUE,
				loop_select!(
					if true => if let 'a' = futures::future::ready('b') => { panic!() } else break BREAK_VALUE,
					if true => let _ = futures::future::pending::<u32>() => {},
				)
			);
		}
	}

	#[tokio::test]
	#[allow(unreachable_code)]
	async fn doesnt_run_disabled_branches() {
		macro_rules! test {
			($({$($branch:tt)+})+) => {
				$({
					let mut stream = futures::stream::iter([(); 4]);

					loop_select!(
						if false => $($branch)+
						if let Some(_) = stream.next() => {},
					);
				})+
			}
		}

		test!({
			let _ = futures::future::ready(3) => {
				panic!();
			},
		}{
			if let Some(_) = futures::future::ready(Some(1)) => {
				panic!();
			},
		}{
			if let None = futures::future::ready(Some(1)) => {
				panic!();
			},
		}{
			if let Some(_) = futures::future::ready(Some(1)) => {
				panic!();
			} else break panic!(),
		}{
			if let None = futures::future::ready(Some(1)) => {
				panic!();
			} else break panic!(),
		});
	}

	#[allow(unreachable_code)]
	#[tokio::test]
	async fn runs_enabled_branches() {
		macro_rules! test {
			($condition_has_run:ident, $branch_has_run:ident, cases: $({$($branches:tt)+})+) => {
				$({
					let mut condition_runs = 0u32;
					let mut branch_runs = 0u32;
					{
						let mut $condition_has_run = || {
							condition_runs += 1;
						};
						let mut $branch_has_run = || {
							branch_runs += 1;
						};
						loop_select!(
							$($branches)+
						);
					}
					assert_eq!(condition_runs, 1);
					assert_eq!(branch_runs, 1);
				})+
			}
		}
		test!(
			condition_has_run,
			branch_has_run,
			cases:
			{
				if true => if let 1 = async {
					condition_has_run();
					1
				} => { branch_has_run(); break },
			}
			{
				if true => let _ = async {
					condition_has_run();
					1
				} => { branch_has_run(); break },
			}
			{
				if true => if let 1 = async {
					condition_has_run();
					1
				} => { branch_has_run(); break; } else break unreachable!(),
			}
			{
				if let 1 = async {
					condition_has_run();
					1
				} => { branch_has_run(); break },
			}
			{
				let _ = async {
					condition_has_run();
					1
				} => { branch_has_run(); break },
			}
			{
				if let 1 = async {
					condition_has_run();
					1
				} => { branch_has_run(); break } else break unreachable!(),
			}
		);
	}
}
