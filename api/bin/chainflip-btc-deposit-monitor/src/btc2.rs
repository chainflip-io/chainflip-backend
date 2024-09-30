use cf_chains::Bitcoin;
use chainflip_engine::{btc::rpc::BtcRpcClient, witness::common::epoch_source::Vault};
use core::str;
use std::sync::{Arc, Mutex};


// bitcoin_endpoint = "tcp://*:8888"

pub async fn monitor_mempool(bitcoin_endpoint: String, vaults: Arc<Mutex<Vec<Vault<Bitcoin, (), ()>>>>) {
    let ctx = zmq::Context::new();

    let socket = ctx.socket(zmq::STREAM).unwrap();
    socket.set_rcvhwm(0).unwrap();
    socket.set_subscribe(b"sequence").unwrap();
    socket.bind(&bitcoin_endpoint).unwrap();
    loop {
        let data = socket.recv_multipart(0).unwrap();
        println!(
            "Identity: {:?} Message : {}",
            data[0],
            str::from_utf8(&data[1]).unwrap()
        );
    }   
}



