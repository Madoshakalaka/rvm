use shared::dep::tokio;
use client::log;


use std::time::Duration;
use shared::dep::{tracing::info, crossterm::{self, execute, terminal::{EnterAlternateScreen, LeaveAlternateScreen}}};


#[tokio::main]
async fn main() {

    crossterm::terminal::enable_raw_mode().unwrap();
    execute!(std::io::stdout(), EnterAlternateScreen).unwrap();

    let (main_end_tx, main_end_rx) = tokio::sync::oneshot::channel::<()>();
    let logging = tokio::spawn(log::start_logging(main_end_rx));


    let run_res = client::run().await;

    info!("Main exit");
    tokio::time::sleep(Duration::from_millis(100)).await;
    main_end_tx.send(()).unwrap();
    logging.await.unwrap().unwrap();


    crossterm::terminal::disable_raw_mode().unwrap();
    execute!(std::io::stdout(), LeaveAlternateScreen).unwrap();

    if run_res.is_err() {
        std::process::exit(-1)
    }
}