use shared::dep::tokio as tokio;

#[tokio::main]
async fn main() {
    if let Err(e) = client::run().await {
        shared::error_util::eprint_error_chain(e);
        std::process::exit(-1);
    };
}