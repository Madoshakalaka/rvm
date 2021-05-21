use std::str::FromStr;

use async_stream::stream;
use crossterm::event::{Event, KeyCode, KeyEvent};
use future::sink::SinkExt;
use futures_util::pin_mut;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::runtime::Handle;
use tokio::task::JoinError;
use tokio::time::Duration;

use shared::deps::async_stream as async_stream;
use shared::deps::futures as future;
use shared::deps::futures_util as futures_util;
use shared::deps::tokio as tokio;
use shared::deps::tokio::sync::broadcast::error::{SendError, RecvError, TryRecvError};
use shared::deps::tokio::task::spawn_blocking;
use shared::deps::tokio_util as tokio_util;
use shared::deps::tokio_util::codec::{AnyDelimiterCodec, Framed};
use shared::message::{CLIENT_QUIT_MESSAGE, ClientToServerMessage, MESSAGE_DELIMITER};

pub mod save;
pub mod custom_error;


use custom_error::{Result, Error};
use tui::backend::CrosstermBackend;
use tui::Terminal;
use std::io::Stdout;
use shared::deps::tokio::sync::broadcast::{Receiver, Sender};
use shared::message::ClientToServerMessage::IWantToDisconnect;
use tui::layout::{Layout, Direction, Constraint};
use tui::widgets::{Block, Borders, Paragraph};
use tui::text::Span;
use std::collections::VecDeque;

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
    while let event = crossterm::event::read()? {
        if let Some(m) = from(event) {
            if m == CLIENT_QUIT_MESSAGE {
                break;
            }

            control_sx.send(m);
        };
    };

    Ok(())
}


pub async fn run() -> Result<()> {
    let (message_tx_from_events, client_message_rx) = tokio::sync::broadcast::channel::<ClientToServerMessage>(16);
    let mut drawer_dispatching_message_rx = message_tx_from_events.subscribe();

    let (debug_message_tx, mut debug_message_rx) = std::sync::mpsc::channel::<String>()  ;


    let mut stdout = std::io::stdout();

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| Error::TerminalInitialization(e))?;

    terminal.clear();

    let draw = tokio::task::spawn_blocking(move || {
        let mut message_buffer:VecDeque<String> = VecDeque::with_capacity(30);
        loop {


            let mut debug_display: Paragraph;


            match drawer_dispatching_message_rx.try_recv(){
                Ok(message) => message_buffer.push_front(message.to_string()),
                Err(TryRecvError::Empty)=> {},
                Err(TryRecvError::Closed)=> {
                    terminal.clear();
                    return Ok::<(), Error>(())},
                Err(TryRecvError::Lagged(_)) => message_buffer.push_front(String::from("Draw thread message receiver lagged"))
            };

            match debug_message_rx.try_recv(){
                Ok(message) => message_buffer.push_front(message.to_string()),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    terminal.clear();
                    return Ok::<(), Error>(())
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            };



            debug_display = match message_buffer.len() {
                0 =>    Paragraph::new("").block(Block::default()
                    .borders(Borders::ALL)),
                1 =>    Paragraph::new(message_buffer[0].clone()).block(Block::default()
                    .borders(Borders::ALL)),
                _ => {
                    message_buffer.truncate(1);
                    Paragraph::new(message_buffer[0].clone()).block(Block::default()
                    .borders(Borders::ALL))}
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


    let mut client: Client = Client::new(client_message_rx, debug_message_tx).await?;

    let send_to_server_handle = tokio::spawn(async move {
        client.forever_send_controls_to_server().await;
        client.disconnect().await;
        ()
    });

    let listen_for_controls_handle =
        tokio::task::spawn_blocking(move || forever_listen_for_controls(message_tx_from_events));


    let (first, second, draw_join) = tokio::join!(send_to_server_handle, listen_for_controls_handle, draw);



    if let Some(join_error) = first.err() {
        return Err(Error::MessageSendingTaskIncomplete(join_error.into()));
    }


    match second {
        Ok(task_result) => task_result?,
        Err(join_error) => return Err(Error::EventListeningTaskIncomplete(join_error.into()))
    }

    match draw_join{
        Ok(draw) => {
            draw?
        },
        Err(join_error) =>{
            return Err(Error::DrawTaskIncomplete(join_error.into()));
        }
    }

    Ok(())
}

struct Client {
    control_rx: Receiver<ClientToServerMessage>,
    debug_message_tx: std::sync::mpsc::Sender<String>,
    framed: Framed<TcpStream, AnyDelimiterCodec>,
}


enum DebugMessage{
    ClientToServer(ClientToServerMessage),
    Client(String)
}

impl Client {

    /// show debug output on screen
    fn debug(&mut self, message: String){
        self.debug_message_tx.send(message);
    }


    /// Tries to connect to the server
    async fn new(control_rx: Receiver<ClientToServerMessage>, mut debug_message_tx: std::sync::mpsc::Sender<String>) -> Result<Client> {
        let stream = TcpStream::connect("127.0.0.1:8080").await
            .map_err(|e| Error::Connection(e))?;

        debug_message_tx.send(String::from("Connected to server"));

        Ok(Self { control_rx,debug_message_tx, framed: shared::message::default_framed(stream) })
    }

    async fn forever_send_controls_to_server(&mut self) {

        // Error<RecvError::Closed> is normal ending
        while let Ok(m) = self.control_rx.recv().await {
            self.send_message(m)
                .await;
        }
        ()
    }

    async fn disconnect(mut self) {
        let message = CLIENT_QUIT_MESSAGE;
        self.debug(format!("Sending {} to server", message.to_string()));

        self.send_message(message).await;
        self.framed.get_mut().shutdown().await;

        // otherwise the stream shutdown too quickly and the server side gets errors - which
        // actually doesn't matter at all except will make the log output ugly.
        tokio::time::sleep(Duration::from_micros(500)).await;
    }

    async fn send_message(&mut self, message: ClientToServerMessage) {
        self.framed.send(message.to_string()).await;
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

