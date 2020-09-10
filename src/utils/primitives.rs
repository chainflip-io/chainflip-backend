use uint::construct_uint;

construct_uint! {
    /// 256-bit unsigned integer.
    pub struct U256(4);
}

construct_uint! {
    /// 512-bit unsigned integer.
    pub struct U512(8);
}
