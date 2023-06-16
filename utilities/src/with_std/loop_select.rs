#[doc(hidden)]
pub use tokio::select as internal_tokio_select;

#[macro_export]
macro_rules! inner_loop_select {
    ({ $($processed:tt)* } let $pattern:pat = $expression:expr => $body:block, $($unprocessed:tt)*) => {
        $crate::inner_loop_select!(
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
    ({ $($processed:tt)* } if let $pattern:pat = $expression:expr => $body:block, $($unprocessed:tt)*) => {
        $crate::inner_loop_select!(
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
    ({ $($processed:tt)* } if let $pattern:pat = $expression:expr => $body:block else break $extra:expr, $($unprocessed:tt)*) => {
        $crate::inner_loop_select!(
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
	({ $($processed:tt)* } if $enable_expression:expr => let $pattern:pat = $expression:expr => $body:block, $($unprocessed:tt)*) => {
        $crate::inner_loop_select!(
            {
                $($processed)*
                x = $expression, if $enable_expression => {
					let $pattern = x;
					$body
				},
            }
            $($unprocessed)*
		)
    };
	({ $($processed:tt)* } if $enable_expression:expr => if let $pattern:pat = $expression:expr => $body:block, $($unprocessed:tt)*) => {
        $crate::inner_loop_select!(
            {
                $($processed)*
                x = $expression, if $enable_expression => {
					if let $pattern = x {
						$body
					} else { break }
				},
            }
            $($unprocessed)*
		)
    };
	({ $($processed:tt)* } if $enable_expression:expr => if let $pattern:pat = $expression:expr => $body:block else break $extra:expr, $($unprocessed:tt)*) => {
        $crate::inner_loop_select!(
            {
                $($processed)*
                x = $expression, if $enable_expression => {
					if let $pattern = x {
						$body
					} else { break $extra }
				},
            }
            $($unprocessed)*
		)
    };
    ({ $($processed:tt)+ }) => {
		loop {
			$crate::internal_tokio_select!(
				$($processed)+
			)
		}
    };
}

#[macro_export]
macro_rules! loop_select {
    ($($cases:tt)+) => {
        $crate::inner_loop_select!({} $($cases)+)
    }
}

#[cfg(test)]
mod test_loop_select {
	use futures::StreamExt;

	#[tokio::test]
	async fn exits_loop_on_branch_failure() {
		// Single branch

		loop_select!(
			if let 'a' = futures::future::ready('b') => { panic!() },
		);
		loop_select!(
			if let 'a' = futures::future::ready('b') => { panic!() } else break 1,
		);
		loop_select!(
			if true => if let 'a' = futures::future::ready('b') => { panic!() },
		);
		loop_select!(
			if true => if let 'a' = futures::future::ready('b') => { panic!() } else break 1,
		);

		// Multiple branches

		// Other branch is Ready
		{
			// Other branch has no enable expression
			loop_select!(
				if let 'a' = futures::future::ready('b') => { panic!() },
				let _ = futures::future::ready('c') => {},
			);
			loop_select!(
				if let 'a' = futures::future::ready('b') => { panic!() } else break 1,
				let _ = futures::future::ready('c') => {},
			);
			loop_select!(
				if true => if let 'a' = futures::future::ready('b') => { panic!() },
				let _ = futures::future::ready('c') => {},
			);
			loop_select!(
				if true => if let 'a' = futures::future::ready('b') => { panic!() } else break 1,
				let _ = futures::future::ready('c') => {},
			);

			// Other branch is disabled
			loop_select!(
				if let 'a' = futures::future::ready('b') => { panic!() },
				if false => let _ = futures::future::ready('c') => {},
			);
			loop_select!(
				if let 'a' = futures::future::ready('b') => { panic!() } else break 1,
				if false => let _ = futures::future::ready('c') => {},
			);
			loop_select!(
				if true => if let 'a' = futures::future::ready('b') => { panic!() },
				if false => let _ = futures::future::ready('c') => {},
			);
			loop_select!(
				if true => if let 'a' = futures::future::ready('b') => { panic!() } else break 1,
				if false => let _ = futures::future::ready('c') => {},
			);

			// Other branch is enabled
			loop_select!(
				if let 'a' = futures::future::ready('b') => { panic!() },
				if true => let _ = futures::future::ready('c') => {},
			);
			loop_select!(
				if let 'a' = futures::future::ready('b') => { panic!() } else break 1,
				if true => let _ = futures::future::ready('c') => {},
			);
			loop_select!(
				if true => if let 'a' = futures::future::ready('b') => { panic!() },
				if true => let _ = futures::future::ready('c') => {},
			);
			loop_select!(
				if true => if let 'a' = futures::future::ready('b') => { panic!() } else break 1,
				if true => let _ = futures::future::ready('c') => {},
			);
		}

		// Other branch is Pending
		{
			// Other branch has no enable expression
			loop_select!(
				if let 'a' = futures::future::ready('b') => { panic!() },
				let _ = futures::future::pending::<u32>() => {},
			);
			loop_select!(
				if let 'a' = futures::future::ready('b') => { panic!() } else break 1,
				let _ = futures::future::pending::<u32>() => {},
			);
			loop_select!(
				if true => if let 'a' = futures::future::ready('b') => { panic!() },
				let _ = futures::future::pending::<u32>() => {},
			);
			loop_select!(
				if true => if let 'a' = futures::future::ready('b') => { panic!() } else break 1,
				let _ = futures::future::pending::<u32>() => {},
			);

			// Other branch is disabled
			loop_select!(
				if let 'a' = futures::future::ready('b') => { panic!() },
				if false => let _ = futures::future::pending::<u32>() => {},
			);
			loop_select!(
				if let 'a' = futures::future::ready('b') => { panic!() } else break 1,
				if false => let _ = futures::future::pending::<u32>() => {},
			);
			loop_select!(
				if true => if let 'a' = futures::future::ready('b') => { panic!() },
				if false => let _ = futures::future::pending::<u32>() => {},
			);
			loop_select!(
				if true => if let 'a' = futures::future::ready('b') => { panic!() } else break 1,
				if false => let _ = futures::future::pending::<u32>() => {},
			);

			// Other branch is enabled
			loop_select!(
				if let 'a' = futures::future::ready('b') => { panic!() },
				if true => let _ = futures::future::pending::<u32>() => {},
			);
			loop_select!(
				if let 'a' = futures::future::ready('b') => { panic!() } else break 1,
				if true => let _ = futures::future::pending::<u32>() => {},
			);
			loop_select!(
				if true => if let 'a' = futures::future::ready('b') => { panic!() },
				if true => let _ = futures::future::pending::<u32>() => {},
			);
			loop_select!(
				if true => if let 'a' = futures::future::ready('b') => { panic!() } else break 1,
				if true => let _ = futures::future::pending::<u32>() => {},
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
			($future:ident, $body:ident, cases: $({$($branches:tt)+})+) => {
				$({
					let mut $future = 0u32;
					let mut $body = 0u32;
					{
						let mut $future = || {
							$future += 1;
						};
						let mut $body = || {
							$body += 1;
						};
						loop_select!(
							$($branches)+
						);
					}
					assert_eq!($future, 1);
					assert_eq!($body, 1);
				})+
			}
		}
		test!(
			future,
			body,
			cases:
			{
				if true => if let 1 = async {
					future();
					1
				} => { body(); break },
			}
			{
				if true => let _ = async {
					future();
					1
				} => { body(); break },
			}
			{
				if true => if let 1 = async {
					future();
					1
				} => { body(); break; } else break unreachable!(),
			}
			{
				if let 1 = async {
					future();
					1
				} => { body(); break },
			}
			{
				let _ = async {
					future();
					1
				} => { body(); break },
			}
			{
				if let 1 = async {
					future();
					1
				} => { body(); break } else break unreachable!(),
			}
		);
	}
}