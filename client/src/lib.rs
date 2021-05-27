use scopeguard::defer;


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
use shared::dep::futures::{StreamExt};


use shared::dep::serde_cbor as serde_cbor;

use shared::dep::tokio;
use shared::dep::tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use shared::dep::tokio::sync::broadcast::Receiver;

use shared::dep::tokio::sync::Mutex;
use shared::dep::tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use shared::message::{CLIENT_QUIT_MESSAGE, ClientToServerMessage, ServerToClientMessage};
use tracing::{debug, info};
use shared::dep::tokio::task::JoinHandle;


pub mod save;
pub mod error;
pub mod log;


#[cfg(test)]
mod tests {
    use super::*;
}


fn forever_listen_for_controls() -> Result<()> {
    loop {
        let event = crossterm::event::read()?;
        if let Some(m) = from(event) {
            let message_is_quit = m == CLIENT_QUIT_MESSAGE;
            debug!("Sending message {:?} to server", m);
            if message_is_quit {
                break;
            }
        };
    }
    Ok(())
}


pub async fn run() -> Result {
    let (_message_tx_from_events, mut client_message_rx) = tokio::sync::broadcast::channel::<ClientToServerMessage>(16);


    let (debug_message_tx, mut debug_message_rx) = tokio::sync::mpsc::channel::<String>(16);

    let (pong_tx, mut pong_rx) = tokio::sync::mpsc::channel::<SystemTime>(16);

    let stdout = std::io::stdout();

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| Error::TerminalInitialization(e))?;

    terminal.clear().map_err(|e| Error::ClearTerminal(e))?;

    let message_buffer: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(Vec::with_capacity(30)));

    let receiving_message_buffer = Arc::clone(&message_buffer);
    let recv_debug = tokio::task::spawn(async move {
        loop {
            match debug_message_rx.recv().await {
                None => { break; }
                Some(m) => {
                    receiving_message_buffer.lock().unwrap().push(m);
                }
            };
        }
        ()
    });

    let message_buffer = Arc::clone(&message_buffer);

    let draw = tokio::task::spawn_blocking(move || {
        let mut frame_count: u8 = 0;
        let mut frame_count_check_point = SystemTime::now();

        info!("Draw loop started.");
        loop {
            let debug_display: Paragraph =

                { // this is the scope for message_buffer lock
                    let mut x = message_buffer.lock().unwrap();

                    frame_count = frame_count.wrapping_add(1);
                    if frame_count == 0 {
                        let now = SystemTime::now();
                        if let Ok(elapsed) = now.duration_since(frame_count_check_point) {
                            x.push(format!("fps: {}", (256 as f32 / elapsed.as_secs_f32()).round()))
                        }

                        frame_count_check_point = now;
                    }

                    match x.len() {
                        0 => {
                            Paragraph::new("").block(Block::default()
                                .borders(Borders::ALL))
                        }
                        1 => {
                            Paragraph::new(x[0].clone()).block(Block::default()
                                .borders(Borders::ALL))
                        }
                        _ => {
                            Paragraph::new(x.pop().unwrap()).block(Block::default()
                                .borders(Borders::ALL))
                        }
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
    });


    let stream = TcpStream::connect("127.0.0.1:8080").await
        .map_err(|e| Error::Connection(e))?;
    let (read_half, write_half) = stream.into_split();

    let framed_write =
        Arc::new(Mutex::new(
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
            r = finally! (info!("Control listening loop ended."), forever_send_controls_to_server(& framed_write, & mut client_message_rx))  =>{
                return r
            }
            r = async {defer!{info!("Ping calculating loop ended.")}; calculate_ping_periodically(& framed_write1, & mut pong_rx, & debug_message_tx).await} =>{
                return r
            }
            r = async {defer! { info!("Server message listening loop ended.")} ; receive_from_server_until_closed(& mut framed_read, pong_tx).await}=>{
                return r
            }
        );
    });

    let listen_for_controls_handle =
        tokio::task::spawn_blocking(move || forever_listen_for_controls());

    return tokio::select!(
        r = finally!(info!("Server interaction tasks ended."), interact_with_server) =>{
            error::from_spawned_task_result(r)
        }
        r = finally!(info!("Stopped watching control events."), listen_for_controls_handle) =>{
            error::from_spawned_task_result(r)
        }
        r = finally!(info!("Stopped drawing"), draw) => {
            error::from_spawned_task_result(r)
        }
        _ = finally!(info!("Stopped receiving on screen debug messages"), recv_debug)  => {
            Ok(())
        }
    ).and_then(|_| {
        info!("Run exits normally");
        Ok(())
    });

}

// todo: move ping display to the top
// todo: use tcp between log windows and main


async fn calculate_ping_periodically(framed_write: &Arc<Mutex<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>>>,
                                     pong_rx: &mut tokio::sync::mpsc::Receiver<SystemTime>,
                                     debug_message_tx: &tokio::sync::mpsc::Sender<String>) -> Result<()> {
    loop {
        send_message(framed_write, ClientToServerMessage::Ping(SystemTime::now())).await?;

        tokio::time::sleep(Duration::from_secs(1)).await;

        if let Some(time) = pong_rx.recv().await {
            debug_message_tx.send(
                format!(
                    "ping: {}",
                    SystemTime::now().sub(Duration::from_secs(1)).duration_since(time).unwrap().as_millis()
                )
            ).await?;
        } else {
            break;
        };
    }

    error::ok()
}


async fn forever_send_controls_to_server(framed_write: &Arc<Mutex<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>>>,
                                         control_rx: &mut Receiver<ClientToServerMessage>, ) -> Result<()> {


    // Error<RecvError::Closed> is normal ending
    while let Ok(m) = control_rx.recv().await {
        send_message(&framed_write, m)
            .await?;
    }

    error::ok()
}


async fn send_message(framed_write: &Arc<Mutex<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>>>,
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

