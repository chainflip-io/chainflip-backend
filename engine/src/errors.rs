// Add a special cool method for adding line numbers
// Ripped from: https://github.com/dtolnay/anyhow/issues/22

macro_rules! here {
    () => {
        format_args!(
            "{}{}",
            concat!("at ", file!(), " line ", line!(), " column ({}) ", column!()),
            lazy_format::lazy_format!("{}", {
                let git_repo_url = core::option_env!("CIRCLE_REPOSITORY_URL");
                let commit_hash = core::option_env!("CIRCLE_SHA1");

                lazy_format::lazy_format!(
                    if git_repo_url.is_some() && commit_hash.is_some() => ("{}/{}#L{}", git_repo_url.unwrap(), commit_hash.is_some(), line!())
                    else => ("")
                )
            })
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
