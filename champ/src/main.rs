mod error;

use error::{Result, Error};
use shared::{dep::{crossterm::{self, event::{EventStream, KeyCode, Event, KeyEvent},
                               terminal::{EnterAlternateScreen, LeaveAlternateScreen}, execute},
                   tokio::{self, net::{TcpStream, TcpListener}, select},
                   tracing::{self, info, error},
                   tokio_util::codec::{FramedRead, LengthDelimitedCodec},
                   futures_util::pin_mut,
                   ansi_term,
                   serde_cbor,
                   futures::{StreamExt, TryFutureExt},
                   futures_core::stream::Stream,
                   tui::{Terminal, backend::CrosstermBackend,
                         layout::{Constraint, Direction, Layout},
                         widgets::{Block, Borders, Paragraph}}},
             message::client_logging::MessageToLogProcess,
};

use tracing_subscriber::filter::LevelFilter;
use std::net::SocketAddr;
use async_stream::try_stream;
use std::str::FromStr;

use shared::dep::tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};


use shared::dep::tokio::sync::RwLock;
use std::time::Duration;
use shared::dep::tokio::time::Interval;
use std::io::Stdout;
use shared::dep::tui::style::Style;
use shared::dep::tui::text::Spans;
use shared::dep::tui::layout::Alignment;


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


async fn handle_connection(incoming_connections: impl Stream<Item=Result<TcpStream>>) {
    pin_mut!(incoming_connections);
    while let Some(x) = incoming_connections.next().await {
        if x.is_err() {
            error!("{:?}", x);
            break;
        }
        let stream: TcpStream = x.unwrap();

        tokio::spawn(async move {
            let framed_read = FramedRead::new(stream,
                                              LengthDelimitedCodec::new());

            let (tx, rx) =
                tokio::sync::mpsc::unbounded_channel::<MessageToLogProcess>();

            return select! {
                r = receive_message_loop(framed_read, tx) => {r}
                _ = print_message_loop(rx) => {error::ok()}
            };
        }.map_err(|x| {
            shared::error_util::report_error_chain(x);
        }));
    }
}


async fn draw_loop(mut frame_duration: Interval,
                   message_buffers: &[RwLock<Vec<String>>; 5],
                   terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Error {
    let mut texts: [Vec<Spans>; 5] = [vec!(), vec!(), vec!(), vec!(), vec!()];


    let mut display_limit: [u8; 5] = [0, 0, 0, 0, 0];

    loop {
        frame_duration.tick().await;


        for (i, message_buffer) in message_buffers.iter().enumerate() {
            let message_buffer = message_buffer.read().await;
            texts[i].clear();
            texts[i].reserve_exact(display_limit[i] as usize);
            texts[i].extend(message_buffer[message_buffer.len().saturating_sub(display_limit[i] as usize)..].iter().map(|x| Spans::from(x.clone())));
        }


        if let Err(e) = terminal.draw(|f| {
            let size = f.size();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Percentage(20),
                        Constraint::Percentage(20),
                        Constraint::Percentage(20),
                        Constraint::Percentage(20),
                        Constraint::Percentage(20)
                    ].as_ref())
                .split(size);

            for (i, chunk) in chunks.iter().enumerate() {
                display_limit[i] = (chunk.height as u8).saturating_sub(2);
            }


            let block = Block::default().style(Style::default());
            f.render_widget(block, size);
            for (i, buffer) in texts.iter().enumerate() {
                let paragraph = Paragraph::new(buffer.clone()).style(Style::default())
                    .block(Block::default()
                        .borders(Borders::ALL)
                        .style(Style::default()))
                    .alignment(Alignment::Left);

                f.render_widget(paragraph, chunks[i]);
            }
        }) {
            return Error::DrawFrame(e);
        };
    };
}


async fn print_message_loop(mut to_be_drawn: UnboundedReceiver<MessageToLogProcess>) -> Result {
    let message_buffs: [RwLock<Vec<String>>; 5] = [RwLock::new(vec!()), RwLock::new(vec!()), RwLock::new(vec!()), RwLock::new(vec!()), RwLock::new(vec!())];

    let message_branching = async {
        while let Some(message) = (&mut to_be_drawn).recv().await {
            &message_buffs[message.level as usize].write().await.push(message.formatted_event);
        }
    };


    let stdout = std::io::stdout();

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| Error::TerminalInitialization(e))?;

    terminal.clear().map_err(|e| Error::ClearTerminal(e))?;

    let frame_duration = tokio::time::interval(Duration::from_secs_f32(1f32 / 60f32));


    return select! {
        _ = message_branching =>{error::ok()}
        e = draw_loop(frame_duration, &message_buffs, & mut terminal) => {Err(e)}
    };
}

async fn receive_message_loop(mut framed_read: FramedRead<TcpStream, LengthDelimitedCodec>,
                              tx: UnboundedSender<MessageToLogProcess>) -> Result {
    while let Some(received_bytes) = framed_read.next().await {
        let received_bytes = received_bytes.map_err(|e| Error::AbruptClientLeave(e))?;

        let received = serde_cbor
        ::from_slice
            ::<MessageToLogProcess>(received_bytes.as_ref())
            .map_err(|e| Error::DeserializationError(e))?;

        tx.send(received).unwrap();
    }
    error::ok()
}


#[tokio::main]
async fn main() {
    crossterm::terminal::enable_raw_mode().unwrap();
    execute!(std::io::stdout(), EnterAlternateScreen).unwrap();

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
    }

    crossterm::terminal::disable_raw_mode().unwrap();
    execute!(std::io::stdout(), LeaveAlternateScreen).unwrap();
}