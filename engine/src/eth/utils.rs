pub use anyhow::Result;
use regex::Regex;
use url::Url;

/// parse the URL and check that it is a valid websocket url
pub(super) fn parse_websocket_url(url: &str) -> Result<Url> {
    let issue_list_url = Url::parse(&url)?;
    if issue_list_url.scheme() != "ws" && issue_list_url.scheme() != "wss" {
        return Err(anyhow::Error::msg("Wrong scheme"));
    }
    if issue_list_url.host() == None
        || issue_list_url.username() != ""
        || issue_list_url.password() != None
        || issue_list_url.query() != None
        || issue_list_url.fragment() != None
        || issue_list_url.cannot_be_a_base()
    {
        return Err(anyhow::Error::msg("Invalid URL data"));
    }

    Ok(issue_list_url)
}

/// checks that the string is formatted as an eth address
pub(super) fn is_eth_address(address: &str) -> Result<()> {
    let re = Regex::new(r"^0x[a-fA-F0-9]{40}$").unwrap();
    match re.is_match(address) {
        true => Ok(()),
        false => Err(anyhow::Error::msg(format!(
            "Invalid Eth Address: {}",
            address
        ))),
    }
}
