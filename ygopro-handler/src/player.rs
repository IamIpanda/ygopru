use std::io::Cursor;

use binrw::BinWrite;
use bytes::Bytes;
use futures::Sink;
use futures::SinkExt;
use futures::Stream;
use futures::StreamExt;
use futures::stream::SplitStream;
use tokio::sync::mpsc;

use ygopro_data::message::stoc;

pub struct Player<Transport> {
    pub name: String,
    pub values: anymap3::Map<dyn std::any::Any + Send + Sync>,

    pub client_to_server_stream: Option<SplitStream<Transport>>,
    pub bytes_sender: mpsc::UnboundedSender<Bytes>,
    pub message_sender: mpsc::UnboundedSender<stoc::Message>,
}

impl<Transport> Player<Transport>
where
    Transport: Stream<Item = Bytes> + Sink<Bytes> + Unpin + Send + 'static,
{
    pub fn new(transport: Transport) -> Self {
        let (mut server_to_client_sink, client_to_server_stream) = transport.split();
        let (bytes_sender, mut bytes_receiver) = mpsc::unbounded_channel();
        let (message_sender, mut message_receiver) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    message = bytes_receiver.recv() => {
                        let Some(message) = message else { break };
                        if server_to_client_sink.send(message).await.is_err() {
                            break;
                        }
                    }
                    message = message_receiver.recv() => {
                        let Some(message): Option<stoc::Message> = message else { break };
                        let mut cursor = Cursor::new(Vec::new());
                        let _ = message.write_le(&mut cursor);
                        let bytes = Bytes::from(cursor.into_inner());
                        if server_to_client_sink.send(bytes).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Self {
            name: String::new(),
            values: Default::default(),
            client_to_server_stream: Some(client_to_server_stream),
            bytes_sender,
            message_sender,
        }
    }
}
