use substrate_api_client::{node_metadata, Api};

/// Start witnessing the state chain
pub fn start() {
    println!("Start the state chain witness");

    let url = "127.0.0.1:9944";
    // let api = Api::<sp_core::sr25519::Pair>::new(format!("ws://{}", url));

    // let meta = api.get_metadata();
    // println!(
    //     "Metadata:\n {}",
    //     node_metadata::pretty_format(&meta).unwrap()
    // );
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn testing_stuff() {
        start();
    }
}
