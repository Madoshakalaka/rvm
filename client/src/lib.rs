use std::collections::VecDeque;


use std::ops::Sub;
use std::sync::Arc;
use std::time::SystemTime;

use bytes::Bytes;
use crossterm::event::{Event, KeyCode, KeyEvent};
use future::sink::SinkExt;

use tokio::net::TcpStream;
use tokio::time::Duration;
use tui::backend::CrosstermBackend;
use tui::layout::{Constraint, Direction, Layout};
use tui::Terminal;
use tui::widgets::{Block, Borders, Paragraph};

use error::{Error, Result};
use shared::dep::bytes as bytes;
use shared::dep::futures as future;
use shared::dep::futures::StreamExt;


use shared::dep::serde_cbor;
use shared::dep::tokio as tokio;
use shared::dep::tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use shared::dep::tokio::sync::broadcast::{Receiver, Sender};
use shared::dep::tokio::sync::broadcast::error::TryRecvError;
use shared::dep::tokio::sync::Mutex;
use shared::dep::tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use shared::message::{CLIENT_QUIT_MESSAGE, ClientToServerMessage, ServerToClientMessage};
use shared::dep::tokio::runtime::task::JoinError;

pub mod save;
pub mod error;


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_serializable() {
        let stringified_variant = ClientToServerMessage::IWantToDisconnect.to_string();
        assert_eq!(stringified_variant, "IWantToDisconnect");

        let parsed_variant: ClientToServerMessage = ClientToServerMessage::from_str("IWantToDisconnect").unwrap();
        assert!(matches!(parsed_variant, ClientToServerMessage::IWantToDisconnect));
    }
}


fn forever_listen_for_controls(control_sx: Sender<ClientToServerMessage>) -> Result<()> {
    loop {
        let event = crossterm::event::read()?;
        if let Some(m) = from(event) {
            if m == CLIENT_QUIT_MESSAGE {
                break;
            }

            control_sx.send(m)?;
        };
    }
    Ok(())
}


pub async fn run() -> Result<()> {
    let (message_tx_from_events, client_message_rx) = tokio::sync::broadcast::channel::<ClientToServerMessage>(16);
    let mut drawer_dispatching_message_rx = message_tx_from_events.subscribe();

    let (debug_message_tx, mut debug_message_rx) = tokio::sync::mpsc::channel::<String>(16);

    let (pong_tx, pong_rx) = tokio::sync::mpsc::channel::<SystemTime>(16);

    let stdout = std::io::stdout();

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| Error::TerminalInitialization(e))?;

    terminal.clear().map_err(|e| Error::ClearTerminal(e))?;

    let message_buffer: Arc<std::sync::Mutex<VecDeque<String>>> = Arc::new(std::sync::Mutex::new(VecDeque::with_capacity(30)));

    let receiving_message_buffer = Arc::clone(&message_buffer);
    let recv_debug = tokio::task::spawn(async move {
        loop {
            match debug_message_rx.recv().await {
                None => { break; }
                Some(m) => {
                    receiving_message_buffer.lock().unwrap().push_front(m);
                }
            };
        }
        ()
    });


    let draw = tokio::task::spawn_blocking(move || {
        loop {
            let debug_display: Paragraph;

            let mut x = message_buffer.lock().unwrap();

            match drawer_dispatching_message_rx.try_recv() {
                Ok(message) => x.push_front(message.to_string()),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Closed) => {
                    break;
                }
                Err(TryRecvError::Lagged(_)) => x.push_front(String::from("Draw thread message receiver lagged"))
            };

            debug_display = match x.len() {
                0 => {
                    Paragraph::new("").block(Block::default()
                        .borders(Borders::ALL))
                }
                1 => {
                    Paragraph::new(x[0].clone()).block(Block::default()
                        .borders(Borders::ALL))
                }
                _ => {
                    x.truncate(1);
                    Paragraph::new(x[0].clone()).block(Block::default()
                        .borders(Borders::ALL))
                }
            };


            terminal.draw(
                |f| {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .margin(1)
                        .constraints(
                            [
                                Constraint::Percentage(10),
                                Constraint::Percentage(80),
                                Constraint::Percentage(10)
                            ].as_ref()
                        )
                        .split(f.size());
                    let block = Block::default()
                        .title("Block")
                        .borders(Borders::ALL);
                    f.render_widget(block, chunks[0]);
                    let block = Block::default()
                        .borders(Borders::ALL);
                    f.render_widget(block, chunks[1]);
                    f.render_widget(debug_display, chunks[2]);
                }
            ).map_err(|err| Error::DrawFrame(err))?;
        }

        terminal.clear().map_err(|e| Error::ClearTerminal(e))?;
        error::ok()
    });


    let stream = TcpStream::connect("127.0.0.1:8080").await
        .map_err(|e| Error::Connection(e))?;
    let (read_half, write_half) = stream.into_split();

    let framed_write =
        Arc::new(Mutex::new(
            FramedWrite::new(write_half, LengthDelimitedCodec::new())
        ));
    let framed_write1 = Arc::clone(&framed_write);

    let framed_read =
        FramedRead::new(read_half, LengthDelimitedCodec::new());

    debug_message_tx.send("Connected to server".parse().unwrap()).await?;

    let mut control_sender = ControlSender::new(framed_write, client_message_rx);
    let mut ping_calculator = PingCalculator::new(framed_write1, pong_rx, debug_message_tx);
    let mut server_reader = ServerReader::new(framed_read, pong_tx);

    let interact_with_server = tokio::spawn(async move {
        tokio::try_join!(
            control_sender.forever_send_controls_to_server(),
            ping_calculator.calculate_ping_periodically(),
            server_reader.receive_from_server_until_closed()
        )?;

        error::ok()
    });

    let listen_for_controls_handle =
        tokio::task::spawn_blocking(move || forever_listen_for_controls(message_tx_from_events));


    match tokio::try_join!(interact_with_server, listen_for_controls_handle, draw, recv_debug) {
        Ok((a, b, c, _)) => {
            a?;
            b?;
            c?;
        }
        Err(e) => Err(Error::MainTaskJoin(e.into()))?
    }


    error::ok()
}

// todo: move ping display to the top
// todo: investigate which task hangs after Esc is pressed
// todo: log to file, maybe add run configuration to watch log file.
//  Wilder idea: add win32 api as dev dependency, open an extra window to display logs.


struct PingCalculator {
    framed_write: Arc<Mutex<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>>>,
    pong_rx: tokio::sync::mpsc::Receiver<SystemTime>,
    debug_message_tx: tokio::sync::mpsc::Sender<String>,
}

impl PingCalculator {
    fn new(framed_write: Arc<Mutex<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>>>,
           pong_rx: tokio::sync::mpsc::Receiver<SystemTime>,
           debug_message_tx: tokio::sync::mpsc::Sender<String>) -> Self {
        Self { framed_write, pong_rx, debug_message_tx }
    }

    async fn calculate_ping_periodically(&mut self) -> Result<()> {
        loop {
            send_message(&self.framed_write, ClientToServerMessage::Ping(SystemTime::now())).await?;

            tokio::time::sleep(Duration::from_secs(1)).await;
            if let Some(time) = self.pong_rx.recv().await {
                self.debug_message_tx.send(
                    format!(
                        "ping: {}",
                        SystemTime::now().sub(Duration::from_secs(1)).duration_since(time).unwrap().as_millis()
                    )
                ).await?;
            } else {
                break;
            }
        }
        error::ok()
    }
}


struct ControlSender {
    framed_write: Arc<Mutex<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>>>,
    control_rx: Receiver<ClientToServerMessage>,
}

impl ControlSender {
    fn new(framed_write: Arc<Mutex<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>>>,
           control_rx: Receiver<ClientToServerMessage>) -> Self {
        Self { framed_write, control_rx }
    }

    async fn forever_send_controls_to_server(&mut self) -> Result<()> {

        // Error<RecvError::Closed> is normal ending
        while let Ok(m) = self.control_rx.recv().await {
            send_message(&self.framed_write, m)
                .await?;
        }
        error::ok()
    }
}


async fn send_message(framed_write: &Arc<Mutex<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>>>,
                      message: ClientToServerMessage) -> Result<()> {
    framed_write.lock().await
        .send(Bytes::from(serde_cbor::to_vec(&message).unwrap())).await
        .map_err(|e| Error::SendMessage(e))?;
    error::ok()
}

struct ServerReader {
    framed_read: FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
    pong_tx: tokio::sync::mpsc::Sender<SystemTime>,
}

impl ServerReader {
    fn new(framed_read: FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
           pong_tx: tokio::sync::mpsc::Sender<SystemTime>) -> Self {
        Self { framed_read, pong_tx }
    }

    async fn receive_from_server_until_closed(&mut self) -> Result<()> {
        while let Some(received_bytes) = self.framed_read.next().await {
            let received_bytes = received_bytes.map_err(|e| Error::DecodeFromServer(e))?;
            let received = serde_cbor::from_slice::<ServerToClientMessage>(&*received_bytes)?;
            match received {
                ServerToClientMessage::Pong(time) => {
                    if let Err(_) = self.pong_tx.send(time).await {
                        break;
                    }
                }
            }
        }
        error::ok()
    }
}


fn from(e: Event) -> Option<ClientToServerMessage> {
    match e {
        Event::Key(KeyEvent { code: KeyCode::Char('w'), modifiers: _none }) => Some(ClientToServerMessage::W),
        Event::Key(KeyEvent { code: KeyCode::Char('p'), modifiers: _none }) => Some(ClientToServerMessage::P),
        Event::Key(KeyEvent { code: KeyCode::Char('a'), modifiers: _none }) => Some(ClientToServerMessage::A),
        Event::Key(KeyEvent { code: KeyCode::Char('s'), modifiers: _none }) => Some(ClientToServerMessage::S),
        Event::Key(KeyEvent { code: KeyCode::Char('d'), modifiers: _none }) => Some(ClientToServerMessage::D),
        Event::Key(KeyEvent { code: KeyCode::Char('j'), modifiers: _none }) => Some(ClientToServerMessage::J),
        Event::Key(KeyEvent { code: KeyCode::Esc, modifiers: _none }) => Some(ClientToServerMessage::IWantToDisconnect),
        Event::Key(KeyEvent { code: KeyCode::Char(' '), modifiers: _none }) => Some(ClientToServerMessage::Spacebar),
        _ => None
    }
}

