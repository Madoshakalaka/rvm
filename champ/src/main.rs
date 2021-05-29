mod error;

use error::{Result, Error};

use futures::StreamExt;

use shared::dep::futures as futures;

use shared::dep::serde_cbor as serde_cbor;
use shared::{dep::{crossterm::event::{EventStream, KeyCode, Event, KeyEvent},
                   tokio::{self, net::{TcpStream, TcpListener}, select},
                   tracing::{self, info, error},
                   tokio_util::codec::{FramedRead, LengthDelimitedCodec},
                   futures_util::pin_mut,
                   ansi_term},
             message::client_logging::MessageToLogProcess};
use tracing_subscriber::filter::LevelFilter;
use std::net::SocketAddr;
use async_stream::try_stream;
use shared::dep::futures_core::stream::Stream;
use shared::dep::futures::TryFutureExt;
use std::str::FromStr;


fn bind_and_accept(addr: SocketAddr)
                   -> impl Stream<Item=Result<TcpStream>>
{
    try_stream! {
        let listener = TcpListener::bind(addr).await.map_err(|e| Error::Bind(e))?;

        loop {
            let (stream, addr) = listener.accept().await.map_err(|e| Error::Accept(e))?;
            info!("received on {:?}", addr);
            yield stream;
        }
    }
}

async fn wait_for_esc() {
    let mut reader = EventStream::new();

    while let Some(Ok(e)) = reader.next().await {
        match e {
            Event::Key(KeyEvent { code: KeyCode::Esc, modifiers: _none }) => { break; }
            _ => {}
        }
    };
}

async fn handle_connection(stream_stream: impl Stream<Item=Result<TcpStream>>) {
    pin_mut!(stream_stream);
    while let Some(x) = stream_stream.next().await {
        if x.is_err() {
            error!("{:?}", x);
            break;
        }
        let stream = x.unwrap();

        tokio::spawn(async move {
            let mut framed_read = FramedRead::new(stream,
                                                  LengthDelimitedCodec::new());


            while let Some(received_bytes) = framed_read.next().await {
                let received_bytes = received_bytes.map_err(|e| Error::AbruptClientLeave(e))?;

                let received = serde_cbor
                ::from_slice
                    ::<MessageToLogProcess>(received_bytes.as_ref())
                    .map_err(|e| Error::DeserializationError(e))?;

                print!("{}", received.formatted_event);
            }
            info!("Client leaves");
            error::ok()
        }.map_err(|x| {
            shared::error_util::report_error_chain(x);
        }));
    }
}


#[tokio::main]
async fn main() {
    let file_appender = tracing_appender::rolling::daily("C:\\logs", "champ.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    let collector = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(LevelFilter::TRACE).with_writer(non_blocking)
        .finish();

    tracing::subscriber::set_global_default(collector)
        .map_err(|_err| eprintln!("Unable to set global default subscriber")).unwrap();

    ansi_term::enable_ansi_support().unwrap();

    let stream_stream = bind_and_accept(SocketAddr::from_str("127.0.0.1:8081").unwrap());


    select! {
        _ = wait_for_esc() => {}
        _ = handle_connection(stream_stream) => {}
        _ = tokio::signal::ctrl_c() => {}
    }
}