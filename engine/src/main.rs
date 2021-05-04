use log::info;

mod mq;
mod p2p;

fn main() {
    // init the logger
    env_logger::init();

    info!("Start your engines!")
}
