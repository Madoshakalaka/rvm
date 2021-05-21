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
use shared::deps::tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use shared::deps::tokio::sync::mpsc::error::SendError;
use shared::deps::tokio::task::spawn_blocking;
use shared::deps::tokio_util as tokio_util;
use shared::deps::tokio_util::codec::{AnyDelimiterCodec, Framed};
use shared::message::{CLIENT_QUIT_MESSAGE, ClientToServerMessage, MESSAGE_DELIMITER};

pub mod custom_error;

use custom_error::{Result, Error};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_serializable() {
        let stringified_variant = ClientToServerMessage::IWantToDisconnect.to_string();
        assert_eq!(stringified_variant, "IWantToDisconnect");

        let parsed_variant: ClientToServerMessage =  ClientToServerMessage::from_str("IWantToDisconnect").unwrap();
        assert!(matches!(parsed_variant, ClientToServerMessage::IWantToDisconnect));
    }
}







fn forever_listen_for_controls(control_sx: UnboundedSender<Event>) -> Result<()>{
    while let Ok(_) = control_sx.send(crossterm::event::read()?){}
    // control_sx fails only when the receiver gets an quit event and closes the
    // the channel
    Ok(())

}

pub async fn run() -> Result<()> {

    let (control_sx, control_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();


    let mut client: Client = Client::new(control_rx).await?;

    let send_to_server_handle = tokio::spawn(async move {
        client.forever_send_controls_to_server().await;
        client.disconnect().await;
    });

    let listen_for_controls_handle =
        tokio::task::spawn_blocking(move || forever_listen_for_controls(control_sx));


    let (first, second) = tokio::join!(send_to_server_handle, listen_for_controls_handle);


    if let Some(join_error) = first.err(){
        return Err(Error::MessageSendingTaskIncomplete(join_error.into()))
    }


    match second{
        Ok(task_result) => task_result?,
        Err(join_error) => return Err(Error::EventListeningTaskIncomplete(join_error.into()))
    }

    Ok(())
}

struct Client {
    control_rx: UnboundedReceiver<Event>,
    framed: Framed<TcpStream, AnyDelimiterCodec>
    // stream: TcpStream,
}


impl Client{
    /// Tries to connect to the server
    async fn new(control_rx: UnboundedReceiver<Event>) -> Result<Client>{
        let stream = TcpStream::connect("127.0.0.1:8080").await?;
        println!("Connected to server");

        Ok(Self{control_rx, framed: shared::message::default_framed(stream)})
    }

    async fn forever_send_controls_to_server(& mut self){

        while let Some(e) = self.control_rx.recv().await{

            if let Some(m) = from(e){

                let is_client_quit_message = m == CLIENT_QUIT_MESSAGE;
                self.send_message(m)
                    .await;
                if is_client_quit_message{
                    self.control_rx.close();
                }
            }
        }

    }

    async fn disconnect(mut self){
        let message = ClientToServerMessage::IWantToDisconnect;
        println!("Sending {} to server", message.to_string());

        self.send_message(message).await;
        self.framed.get_mut().shutdown().await;

        // otherwise the stream shutdown too quickly and the server side gets errors - which
        // actually doesn't matter at all except will make the log output ugly.
        tokio::time::sleep(Duration::from_micros(500)).await;
    }

    async fn send_message(&mut self, message:ClientToServerMessage) {
        // let stream = ClientToServerMessage::with_pending_delimiter(message);
        // pin_mut!(stream);
        // self.framed.send_all(& mut stream);
        // self.stream.write_all((message.to_string()).as_ref()).await;
        // self.stream.write(MESSAGE_DELIMITER).await;
        self.framed.send(message.to_string()).await;
    }
}


fn from(e: Event) -> Option<ClientToServerMessage> {
    match e {
        Event::Key(KeyEvent{code:KeyCode::Char('w'), modifiers:_none}) => Some(ClientToServerMessage::W),
        Event::Key(KeyEvent{code:KeyCode::Char('p'), modifiers:_none}) => Some(ClientToServerMessage::P),
        Event::Key(KeyEvent{code:KeyCode::Char('a'), modifiers:_none}) => Some(ClientToServerMessage::A),
        Event::Key(KeyEvent{code:KeyCode::Char('s'), modifiers:_none}) => Some(ClientToServerMessage::S),
        Event::Key(KeyEvent{code:KeyCode::Char('d'), modifiers:_none}) => Some(ClientToServerMessage::D),
        Event::Key(KeyEvent{code:KeyCode::Char('j'), modifiers:_none}) => Some(ClientToServerMessage::J),
        Event::Key(KeyEvent{code:KeyCode::Esc, modifiers:_none}) => Some(ClientToServerMessage::IWantToDisconnect),
        Event::Key(KeyEvent{code:KeyCode::Char(' '), modifiers:_none}) => Some(ClientToServerMessage::Spacebar),
        _ => None
    }
}

