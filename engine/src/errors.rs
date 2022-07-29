// Add a special cool method for adding line numbers
// Ripped from: https://github.com/dtolnay/anyhow/issues/22

macro_rules! here {
    () => {
        format_args!(
            "{}{}",
            concat!("at ", file!(), " line ", line!(), " column ", column!()),
            lazy_format::lazy_format!(
                if let Some(commit_hash) = core::option_env!("CIRCLE_SHA1") => (" [link](https://github.com/chainflip-io/chainflip-backend/tree/{}#L{})", commit_hash, line!())
                else => ("")
            )
        )
    };
}

macro_rules! context {
    ($e:expr) => {{
        // Using function ensures the expression's temporary's lifetimes last until after context!() call
        #[inline(always)]
        fn get_expr_type<V, E, T: anyhow::Context<V, E>, Here: core::fmt::Display>(
            t: T,
            here: Here,
        ) -> anyhow::Result<V> {
            t.with_context(|| {
                format!(
                    "Error: '{}' with type '{}' failed {}",
                    stringify!($e),
                    std::any::type_name::<T>(),
                    here
                )
            })
        }

        get_expr_type($e, here!())
    }};
}
