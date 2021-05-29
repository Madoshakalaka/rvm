use scopeguard::defer;


use std::ops::Sub;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use bytes::Bytes;

use future::sink::SinkExt;

use tokio::net::TcpStream;
use tokio::time::Duration;
use tokio::sync::Mutex as TMutex;
use tui::backend::CrosstermBackend;
use tui::layout::{Constraint, Direction, Layout};
use tui::Terminal;
use tui::widgets::{Block, Borders, Paragraph};

use error::{Error, Result};
use shared::dep::bytes as bytes;
use shared::dep::futures as future;
use shared::dep::futures::StreamExt;


use shared::dep::serde_cbor as serde_cbor;

use shared::dep::{tokio, tracing, crossterm::event::{Event, KeyCode, KeyEvent, EventStream}};
use shared::dep::tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use shared::dep::tokio::sync::broadcast::Receiver;


use shared::dep::tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use shared::message::server_client::{ClientToServerMessage, ServerToClientMessage};
use tracing::{debug, info};
use shared::dep::tokio::task::JoinHandle;


use std::io::Stdout;
use std::collections::VecDeque;
use std::convert::TryFrom;
use crate::error::PingError;


pub mod save;
pub mod error;
pub mod log;


#[cfg(test)]
mod tests {
    use super::*;
}


async fn forever_listen_for_controls(message_tx_from_events: tokio::sync::broadcast::Sender<ClientToServerMessage>)
                                     -> Result<()> {
    let mut reader = EventStream::new();

    while let Some(Ok(e)) = reader.next().await {
        if let Some(m) = from(e) {
            debug!("Sending message {:?} to server", m);
            message_tx_from_events.send(m).unwrap();
        }
    }
    Ok(())
}

async fn draw_frame_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>,
                         mut calculated_ping_rx: tokio::sync::mpsc::Receiver<std::result::Result<u16, PingError>>) -> Result {
    let lower_screen_message_queue: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::with_capacity(30)));

    let mut frame_duration = tokio::time::interval(Duration::from_secs_f32(1f32 / 60f32));
    let mut frame_count: u8 = 0;
    let mut frame_count_check_point = SystemTime::now();

    info!("Draw loop started.");
    let mut lower_screen_buffer = String::from("");

    let message_queue_pusher = Arc::clone(&lower_screen_message_queue);
    let ping_recv = async move {
        loop {
            if let Some(Ok(x)) = calculated_ping_rx.recv().await {
                message_queue_pusher.lock().unwrap().push_front(format!("ping: {}", x));
            }
        }
    };

    let draw_loop = async move {
        loop {
            frame_duration.tick().await;

            frame_count = frame_count.wrapping_add(1);


            { // block for the mutex guard
                let mut lower_screen_message_queue = lower_screen_message_queue.lock().unwrap();
                if frame_count == 0 {
                    let now = SystemTime::now();
                    if let Ok(elapsed) = now.duration_since(frame_count_check_point) {
                        lower_screen_message_queue.push_front(format!("fps: {}", (256 as f32 / elapsed.as_secs_f32()).round()))
                    }

                    frame_count_check_point = now;
                }

                if let Some(message) = lower_screen_message_queue.pop_back() {
                    lower_screen_buffer.clear();
                    lower_screen_buffer.push_str(&*message);
                }
            }


            let debug_display: Paragraph = Paragraph::new(&lower_screen_buffer[..]).block(Block::default()
                .borders(Borders::ALL));


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
    };

    tokio::select!(
        _ = ping_recv => {
            Ok(())
        }
        r = draw_loop => {
            r
        }
    )
}


pub async fn run() -> Result {
    let (message_tx_from_events, mut client_message_rx) = tokio::sync::broadcast::channel::<ClientToServerMessage>(16);


    let (calculated_ping_tx, calculated_ping_rx) = tokio::sync::mpsc::channel::<std::result::Result<u16, PingError>>(16);

    let (pong_tx, mut pong_rx) = tokio::sync::mpsc::channel::<SystemTime>(16);

    let stdout = std::io::stdout();

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| Error::TerminalInitialization(e))?;

    terminal.clear().map_err(|e| Error::ClearTerminal(e))?;


    let stream = TcpStream::connect("127.0.0.1:8080").await
        .map_err(|e| Error::Connection(e))?;
    let (read_half, write_half) = stream.into_split();

    let framed_write =
        Arc::new(TMutex::new(
            FramedWrite::new(write_half, LengthDelimitedCodec::new())
        ));
    let framed_write1 = Arc::clone(&framed_write);

    let mut framed_read =
        FramedRead::new(read_half, LengthDelimitedCodec::new());

    info!("Connected to server");

    info!("Control listening loop started.");
    info!("Ping calculating loop started.");
    info!("Server message listening loop started.");

    let interact_with_server: JoinHandle<Result> = tokio::spawn(async move {
        tokio::select!(
            r = finally_info! ("Control listening loop ended.", forever_send_controls_to_server(& framed_write, & mut client_message_rx))  =>{
                return r
            }
            r = finally_info!("Ping calculating loop ended.", calculate_ping_periodically(& framed_write1, & mut pong_rx, & calculated_ping_tx)) =>{
                return r
            }
            r = finally_info!("Server message listening loop ended.", receive_from_server_until_closed(& mut framed_read, pong_tx)) =>{
                return r
            }
        );
    });

    let draw = draw_frame_loop(&mut terminal, calculated_ping_rx);

    return tokio::select!(
        r = finally_info!("Server interaction tasks ended.", interact_with_server) =>{
            error::from_spawned_task_result(r)
        }
        r = finally_info!("Stopped watching control events.", forever_listen_for_controls(message_tx_from_events)) =>{
            r
        }
        r = finally_info!("Stopped drawing", draw) => {
            r
        }
    ).and_then(|_| {
        info!("Run exits normally");
        (&mut terminal).clear().unwrap();
        Ok(())
    });
}

// todo: move ping display to the top
// todo: use tcp between log windows and main
// todo: properly exit


async fn calculate_ping_periodically(framed_write: &Arc<TMutex<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>>>,
                                     pong_rx: &mut tokio::sync::mpsc::Receiver<SystemTime>,
                                     calculated_ping_tx: &tokio::sync::mpsc::Sender<std::result::Result<u16, PingError>>) -> Result<()> {
    loop {
        send_message(framed_write, ClientToServerMessage::Ping(SystemTime::now())).await?;

        tokio::time::sleep(Duration::from_secs(1)).await;

        if let Some(time) = pong_rx.recv().await {
            let message: std::result::Result<u16, PingError>;

            match SystemTime::now().sub(Duration::from_secs(1)).duration_since(time) {
                Ok(diff) => {
                    message = u16::try_from(diff.as_millis()).map_err(|e| e.into());
                }
                Err(e) => {
                    message = Err(PingError::TimeTraveler(e));
                }
            }
            calculated_ping_tx.send(message).await.unwrap();
        } else {
            break;
        };
    }

    error::ok()
}


async fn forever_send_controls_to_server(framed_write: &Arc<TMutex<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>>>,
                                         control_rx: &mut Receiver<ClientToServerMessage>, ) -> Result<()> {


    // Error<RecvError::Closed> is normal ending
    while let Ok(m) = control_rx.recv().await {
        send_message(&framed_write, m)
            .await?;
    }

    error::ok()
}


async fn send_message(framed_write: &Arc<TMutex<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>>>,
                      message: ClientToServerMessage) -> Result<()> {
    framed_write.lock().await
        .send(Bytes::from(serde_cbor::to_vec(&message).unwrap())).await
        .map_err(|e| Error::SendMessage(e))?;
    error::ok()
}

async fn receive_from_server_until_closed(framed_read: &mut FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
                                          pong_tx: tokio::sync::mpsc::Sender<SystemTime>, ) -> Result<()> {
    while let Some(received_bytes) = framed_read.next().await {
        let received_bytes = received_bytes.map_err(|e| Error::DecodeFromServer(e))?;
        let received = serde_cbor::from_slice::<ServerToClientMessage>(&*received_bytes)?;
        debug!("Received {:?} from server", received);
        match received {
            ServerToClientMessage::Pong(time) => {
                if let Err(_) = pong_tx.send(time).await {
                    break;
                }
            }
            ServerToClientMessage::DisconnectAcknowledged => {}
        }
    }
    error::ok()
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

