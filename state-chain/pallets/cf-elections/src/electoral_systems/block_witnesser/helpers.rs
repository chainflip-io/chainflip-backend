
#[macro_export]
macro_rules! prop_do {
    (let $var:pat in $expr:expr; $($expr2:tt)+) => {
        $expr.prop_flat_map(move |$var| prop_do!($($expr2)+))
    };
    (return $($rest:tt)+) => {
        Just($($rest)+)
    };
	($ctor:ident {
		$($field:ident : $strat:expr,)*
	}) => {
		( $($strat,)* ).prop_map(
			|($($field,)*)| $ctor {
				$($field,)*
			}
		)
	};
    ($expr:expr) => {$expr};
    (let $var:pat = $expr:expr; $($expr2:tt)+ ) => {
        {
            let $var = $expr;
            prop_do!($($expr2)+)
        }
    };
    ($var:ident <<= $expr:expr; $($expr2:tt)+) => {
        $expr.prop_flat_map(move |$var| prop_do!($($expr2)+))
    };
}

#[macro_export]
macro_rules! asserts {
	($description:tt in $expr:expr; $($tail:tt)*) => {
		assert!($expr, $description);
		asserts!{$($tail)*}
	};
	($description:tt in $expr:expr, else $($vars:expr), +; $($tail:tt)*) => {
		assert!($expr, $description, $($vars), +);
		asserts!{$($tail)*}
	};
	($description:tt in $expr:expr, where {$($tt:tt)*} $($tail:tt)*) => {
		{
			$($tt)*
			assert!($expr, $description);
		}
		asserts!{$($tail)*}
	};
	(let $ident:ident = $expr:expr; $($tail:tt)*) => {
		let $ident = $expr;
		asserts!{$($tail)*}
	};
	() => {}
}
