use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender, UnboundedReceiver};
use tokio::time::delay_for;
use tokio::time::{timeout, Duration};
use tokio::io::{ReadHalf, WriteHalf};

use byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt};
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};
use std::net::Ipv6Addr;
use sv::messages::{
    Ping,
    Version, NodeAddr, Message, MessageHeader
};
use sv::network::Network;
use cryptoxide::{
    digest::Digest,
    sha2::Sha256
};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

enum PeerMessage {
    Message(Message),
    Stop
}

pub trait MessageHandle {
    fn handle_message() {

    }
}

pub struct Peer {
    addr: String,
    runing: Arc<AtomicBool>,
}

impl Peer {
    pub async fn connect(addr: &str) -> Result<Peer, Box<dyn Error>> {
        let mut stream = TcpStream::connect(addr).await?;
        let (mut reader, mut writer) = tokio::io::split(stream);
        let (mut tx, mut rx) = unbounded_channel::<PeerMessage>();

        let peer = Peer {
            addr: addr.into(),
            runing: Arc::new(AtomicBool::new(true)),
        };

        tokio::spawn(Self::start_background_task(
            rx,
            writer,
        ));

        tokio::spawn(Self::start_ping_task(peer.runing.clone(), tx.clone()));

        tokio::spawn(Self::run(peer.runing.clone(), tx, reader));

        Ok(peer)
    }

    async fn start_background_task(mut rx: UnboundedReceiver<PeerMessage>,  mut writer: WriteHalf<TcpStream>) {
        let magic = Network::Mainnet.magic();
        while let Some(message) = rx.recv().await {
            match message {
                PeerMessage::Message(msg) => {
                    // println!("got = {:?}", message);
                    let mut ss = vec![];
                    msg.write(&mut ss, magic);
                    writer.write(&ss).await;
                },
                PeerMessage::Stop => {
                    println!("stop message");
                    break;
                },
                _ => {

                }
            }
        }
    }

    async fn start_ping_task(runing: Arc<AtomicBool>, tx: UnboundedSender<PeerMessage>) {
        loop {
            if runing.load(Ordering::Relaxed) == false {
                break;
            }

            delay_for(Duration::from_millis(1000 * 30)).await;
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            tx.send(PeerMessage::Message(Message::Ping(Ping{
                nonce: now as u64,
            })));
        }
    }

    async fn run(runing: Arc<AtomicBool>, mut tx: UnboundedSender<PeerMessage>, mut reader: ReadHalf<TcpStream>) {

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        let version = Version {
            version: 70012,
            services: 0,
            timestamp: now as i64,
            recv_addr: NodeAddr{
                services: 0,
                ip: Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1),
                port: 0,
            },
            tx_addr: NodeAddr{
                services: 0,
                ip: Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1),
                port: 0,
            },
            nonce: 1787878,
            user_agent: "/rust.sv:12.0 by tj".into(),
            start_height: 0,
            relay: true,
        };
        tx.send(PeerMessage::Message(Message::Version(version)));

        let mut buf = [0; 10240];
        let mut read_buf: Vec<u8> = vec![];

        let mut header_read = false;
        let mut message_header: Option<MessageHeader> = None;

        loop {
            println!("start reading");
            let n = match reader.read(&mut buf).await {
                Ok(n) if n== 0 => {
                    println!("no data");
                    break;
                },
                Ok(n) => n,
                Err(e) => {
                    tx.send(PeerMessage::Stop);
                    runing.store(false, Ordering::Relaxed);
                    eprintln!("failed to read from socket; err = {:?}", e);
                    break;
                }
            };

            println!("read {} bytes.", n);
            read_buf.extend(&buf[0..n]);

            if header_read == false {

                if read_buf.len() < 24 {
                    continue;
                }

                let mut rdr = Cursor::new(&read_buf[0..24]);

                let mut header = MessageHeader {
                    ..Default::default()
                };

                rdr.read(&mut header.magic).await;
                rdr.read(&mut header.command).await;
                header.payload_size = ReadBytesExt::read_u32::<LittleEndian>(&mut rdr).unwrap();
                rdr.read(&mut header.checksum).await;

                println!("read header {:?}", header);
                message_header = Some(header);

                header_read = true;
                read_buf = read_buf[24..].to_vec();
            }

            if header_read == true {
                let header = message_header.as_ref().unwrap();
                let payload_size = header.payload_size as usize;
                if read_buf.len() < payload_size {
                    continue;
                }

                let mut rdr = Cursor::new(&read_buf[0..payload_size]);
                let message = Message::read_partial(&mut rdr, &header);
                header_read = false;
                println!("{:?}", message);
                read_buf = read_buf[payload_size..].to_vec();

                match message {
                    Ok(Message::Version(v)) => {
                        tx.send(PeerMessage::Message(Message::Verack));
                    },

                    Ok(Message::Verack) => {
                        // request Mempool transactions
                        // tx.send(Message::Mempool);
                        tx.send(PeerMessage::Message(Message::GetAddr));
                    },

                    Ok(Message::Inv(inv)) => {
                        tx.send(PeerMessage::Message(Message::GetData(inv)));
                    },

                    Ok(Message::Ping(ping)) => {
                        tx.send(PeerMessage::Message(Message::Pong(ping)));
                    },

                    _ => {

                    }
                };
            }
        }
    }
}
