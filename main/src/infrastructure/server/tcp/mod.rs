use crate::infrastructure::server::grpc::proto::{ClipPositionUpdate, GetClipPositionUpdatesReply};
use crossbeam_channel::Receiver;
use prost::Message;
use std::error::Error;
use std::io::Write;
use std::net::{TcpListener, TcpStream};

pub fn start_tcp_server(receiver: Receiver<GetClipPositionUpdatesReply>) {
    std::thread::spawn(move || {
        let listener = TcpListener::bind("127.0.0.1:39180").unwrap();
        for stream in listener.incoming() {
            println!("Sending to new client...");
            let stream = stream.unwrap();
            let _ = read(stream, &receiver);
        }
    });
}

fn read(
    mut stream: TcpStream,
    receiver: &Receiver<GetClipPositionUpdatesReply>,
) -> Result<(), Box<dyn Error>> {
    for updates in receiver.iter() {
        let buffer = updates.encode_length_delimited_to_vec();
        stream.write_all(&buffer)?;
        stream.flush()?;
    }
    Ok(())
}
