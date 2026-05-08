use std::fmt::{Debug, Display};
use std::net::SocketAddr;

use log::{trace, info, error};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt, AsyncRead, AsyncWrite};
use ygopro::message::{client_to_server, server_to_client, ref_slicer};

use crate::config::CONFIG;

pub async fn run_proxy() {
    let server_addr: SocketAddr = format!("{}:{}", "0.0.0.0", CONFIG.port).parse().expect("Cannot parse the listening socket.");
    let client_listener = TcpListener::bind(server_addr).await.expect("Failed to bind the port");
    loop {
        let (client_socket,client_addr) = client_listener.accept().await.expect("Cannot get listen socket");
        let server_socket = TcpStream::connect(CONFIG.target).await.expect("Cannot get send socket");
        let (client_reader, client_writer) = client_socket.into_split();
        let (server_reader, server_writer) = server_socket.into_split();
        info!("{:} <-> {:} ", client_addr, CONFIG.target);
        spawn_task(client_reader, server_writer, move |data: &[u8]| {
            let (messages, _) = client_to_server::MessageComplex::from_slice(data, ref_slicer);
            let len = messages.len();
            for (id, message) in messages.into_iter().enumerate() {
                match message.message_enum() {
                    Ok(message_enum) => log_message("C→", message_enum, message.as_ref(), id+1, len),
                    Err(err) => {
                        error!("error on {:}/{:}, error: {:?}\n, data: {:?}", id+1, len, err, &message)
                    }
                }
            }
        });
        spawn_task(server_reader, client_writer, move |data| {
            let (messages, _) = server_to_client::MessageComplex::from_slice(data, ref_slicer);
            let len = messages.len();
            for (id, message) in messages.into_iter().enumerate() {
                match message.message_enum() {
                    Ok(message_enum) => log_message("←S", message_enum, message.as_ref(), id+1, len),
                    Err(err) => {
                        error!("error on {:}/{:}, error: {:?}\n, data: {:?}", id+1, len, err, &message)
                    }
                }
            }
        });
    };
}

fn spawn_task(
    mut reader: impl AsyncRead + Unpin + Send + 'static, 
    mut writer: impl AsyncWrite + Unpin + Send + 'static, 
    log: impl for<'a> Fn(&'a [u8]) -> () + Send + 'static) 
{
    tokio::spawn(async move {
        let mut buf = [0u8; 10240];
        loop {
            let data = match recieve(&mut buf, &mut reader).await {
                Ok(Some(data)) => data,
                Ok(None) => break,
                Err(_) => continue
            };
            log(data);
            writer.write_all(data).await.ok();
        }
    });
    info!("Link dropped.");
}

fn log_message<T: Debug>(leader: &str, message: T, bytes: &[u8], id: impl Display, len: impl Display) { 
    info!("[{:}]({:2}/{:2}) {:?}", leader, id, len, message);
    trace!("[{:}]({:2}/{:2}) {:?}", leader, id, len, bytes)
}


pub async fn recieve<'bytes>(buffer: &'bytes mut [u8], mut reader: impl AsyncRead + Unpin) -> std::io::Result<Option<&'bytes [u8]>> {
    let mut pos = 0;
    loop {
        let n = {
            let current_buffer = &mut buffer[pos..];
            let n = reader.read(current_buffer).await?;
            if n == 0 { return Ok(None); }
            if n > current_buffer.len() { return Err(std::io::ErrorKind::OutOfMemory.into()); }
            n
        };
        pos += n;
        let data = &buffer[0..pos];
        if ygopro::message::is_data_full(data) { break };
    }
    return Ok(Some(&buffer[0..pos]));
}
