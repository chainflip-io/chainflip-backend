macro_rules! blanket_impl {
    ( $trait: ident, $($provides: ident),* ) => {
        pub trait $trait where $(Self: $provides),* {}
        impl<T> $trait for T where $(T: $provides),* {}
    };
}
