use base58_monero;

/// Get the integrated address from a `base_address` and a `payment_id`.
///
/// # Example
///
/// ```
/// use chainflip::utils::oxen::address::get_integrated_address;
///
/// let base_address = "L7fffztMU6PF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RGxxHWxK";
/// let payment_id: [u8; 8] = [66, 15, 162, 155, 45, 154, 73, 245];
/// let integrated_address = get_integrated_address(base_address, &payment_id);
///
/// assert_eq!(integrated_address.unwrap(), "LHNLgohr5MuF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RQdP3TqmQTySUkKMhZf".to_owned());
/// ```
pub fn get_integrated_address(
    base_address: &str,
    payment_id: &[u8; 8],
) -> Result<String, &'static str> {
    let decoded_address =
        base58_monero::decode_check(base_address).map_err(|_| "Failed to decode base address")?;

    if decoded_address.is_empty() {
        return Err("Decoded address is empty");
    }

    let integrated_address_tag = get_integrated_address_tag(decoded_address[0])?;

    let mut buffer = Vec::from(decoded_address);
    buffer[0] = integrated_address_tag;
    buffer.extend(payment_id);

    base58_monero::encode_check(&buffer).map_err(|_| "Failed to create integrated address")
}

/// Get the address tag for an integrated address regardless of network type
fn get_integrated_address_tag(from: u8) -> Result<u8, &'static str> {
    // Oxen information from: https://docs.loki.network/Wallets/Addresses/MainAddress/
    // Monero information from: https://monerodocs.org/public-address/standard-address/
    match from {
        // Oxen - main net (main, integrated, subaddress)
        114 | 115 | 116 => Ok(115),
        // Oxen - stage net (main, integrated, subaddress)
        24 | 25 | 36 => Ok(25),
        // Oxen - test net (main, integrated, subaddress)
        156 | 157 | 158 => Ok(157),
        // Monero - main net (main, integrated, subaddress)
        18 | 19 | 42 => Ok(19),
        // Monero - test net (main, integrated, subaddress)
        53 | 54 | 63 => Ok(54),
        // We don't have enough information of other coins
        _ => Err("Cannot determine integrated address tag"),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // 420fa29b2d9a49f5
    const PAYMENT_ID: [u8; 8] = [66, 15, 162, 155, 45, 154, 73, 245];

    #[test]
    fn returns_oxen_integrated_address() {
        let address = "L7fffztMU6PF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RGxxHWxK";
        let expected = "LHNLgohr5MuF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RQdP3TqmQTySUkKMhZf";

        let integrated_address = get_integrated_address(address, &PAYMENT_ID);
        assert_eq!(integrated_address, Ok(expected.to_owned()));
    }

    #[test]
    fn returns_oxen_test_net_integrated_address() {
        let address = "T6SvvzhYyo2cUwiZBtLoTKGBSqoeGYKP12nsJx3ZsHNm7NhLDwYezTU3Ya9Cgb1UgW3gZTE5RG5ny4QKTUbHiXS8267AzhpZs";
        let expected = "TG9bwoX3b4YcUwiZBtLoTKGBSqoeGYKP12nsJx3ZsHNm7NhLDwYezTU3Ya9Cgb1UgW3gZTE5RG5ny4QKTUbHiXS8NCR92bBy6zV1dq77maK4";

        let integrated_address = get_integrated_address(address, &PAYMENT_ID);
        assert_eq!(integrated_address, Ok(expected.to_owned()));
    }

    #[test]
    fn returns_error_if_invalid_address() {
        assert!(get_integrated_address("Address", &PAYMENT_ID).is_err());
    }
}
