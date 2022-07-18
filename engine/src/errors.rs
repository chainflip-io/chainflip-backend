// Add a special cool method for adding line numbers
// Ripped from: https://github.com/dtolnay/anyhow/issues/22

macro_rules! here {
    () => {
        concat!("at ", file!(), " line ", line!(), " column ", column!())
    };
}

macro_rules! context {
    ($e:expr) => {{
        // Using function ensures the expression's temporary's lifetimes last until after context!() call
        #[inline(always)]
        fn get_expr_type<V, E, T: anyhow::Context<V, E>>(
            t: T,
            here: &'static str,
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
