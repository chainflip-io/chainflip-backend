use chainflip_engine::{eth, sc_observer, settings::Settings};

#[tokio::main]
async fn main() {
    // init the logger
    env_logger::init();

    let settings = Settings::new().expect("Failed to initialise settings");

    log::info!("Start the engines! :broom: :broom: ");

    sc_observer::sc_observer::start(settings.clone()).await;

    eth::start(settings.clone()).await;
}
