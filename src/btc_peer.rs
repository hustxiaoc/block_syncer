use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::sync::{Mutex, RwLock};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender, UnboundedReceiver};
use tokio::time::delay_for;
use tokio::time::{timeout, Duration};
use tokio::io::{ReadHalf, WriteHalf};

// use byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt};
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};
use std::net::Ipv6Addr;
use sv::messages::{
    Ping,
    Inv,
    BlockHeader,
    BlockLocator,
    NO_HASH_STOP,
    TxIn, TxOut, Tx, OutPoint,
    INV_VECT_TX,
    INV_VECT_BLOCK, InvVect,
    Version, NodeAddr, MessageHeader
};
use sv::script::Script;
use sv::util::{Hash256, Amount};
use sv::address::{addr_encode, AddressType};
use sv::transaction::p2pkh::{extract_pubkeyhash};
use sv::network::Network;
use cryptoxide::{
    digest::Digest,
    sha2::Sha256
};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{self, AsyncReadExt};
use tokio_byteorder::{LittleEndian, AsyncWriteBytesExt, AsyncReadBytesExt};
use std::collections::HashSet;

use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr};
use std::{env, process};
use btc::consensus::encode;
use btc::network::{message_blockdata::Inventory, address, constants, message::{self, RawNetworkMessage, NetworkMessage as Message}, message_network};
use btc::network::stream_reader::StreamReader;
use btc::secp256k1;

const init_block_header: &str = "00000000000000000208b2921839605f12b7969019ac14592e33c924d1cc0865";
const first_block_header: &str = "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048";

macro_rules! read_varint_num {
    ($e:expr) => {  
        {
            let first = AsyncReadBytesExt::read_u8($e).await?;
            match first {
                0xFD => {
                    let n = AsyncReadBytesExt::read_u16::<LittleEndian>($e).await?;
                    n as u64
                },

                0xFE => {
                    let n = AsyncReadBytesExt::read_u32::<LittleEndian>($e).await?;
                    n as u64
                },

                0xFF => {
                    AsyncReadBytesExt::read_u64::<LittleEndian>($e).await?
                },

                _ => {
                    first as u64
                }
            }
        }
    };
}

macro_rules! drain_buf {
    ($reader:expr, $len: expr, $buf_len: expr) => {
        let mut read = 0;
        loop {
            let mut need_read = $buf_len;
            let remain = $len - read;
            if remain < need_read {
                need_read = remain;
            }

            if need_read == 0 {
                break;
            }
            let mut buffer = vec![0; need_read as usize];
            $reader.read_exact(buffer.as_mut_slice()).await?;
            read = read + need_read;
        }
    }
}

#[derive(Debug)]
enum PeerMessage {
    Message(Message),
    Stop
}

pub trait MessageHandle {
    fn handle_message() {

    }
}

pub struct Peer {
    addr: NodeAddr,
    runing: Arc<AtomicBool>,
    watch_addrs: Arc<RwLock<HashSet<String>>>,
    blocks: Arc<Mutex<HashSet<String>>>,
    addr_tx: UnboundedSender<NodeAddr>,
    tx_sender: UnboundedSender<(Tx, String)>,
    tx: Option<UnboundedSender<PeerMessage>>,
}

impl Peer {
    pub fn new(
        addr: NodeAddr, watch_addrs: Arc<RwLock<HashSet<String>>>, 
        addr_tx: UnboundedSender<NodeAddr>, 
        blocks: Arc<Mutex<HashSet<String>>>,
        tx_sender: UnboundedSender<(Tx, String)>,
    ) -> Self {

        let peer = Peer {
            blocks,
            addr,
            watch_addrs,
            addr_tx,
            tx_sender,
            runing: Arc::new(AtomicBool::new(true)),
            tx: None,
        };

        peer
    }


    pub async fn connect(&mut self) -> Result<(), Box<dyn Error>> {

        let (tx,  rx) = unbounded_channel::<PeerMessage>();

        let addr = std::net::SocketAddr::new(std::net::IpAddr::V6(self.addr.ip), self.addr.port);
        let stream = TcpStream::connect(addr).await?;
        let (reader, writer) = tokio::io::split(stream);

        let tx_clone = tx.clone();

        self.tx = Some(tx);    

        tokio::spawn(async move {
            match Peer::start_background_task(rx, writer).await {
                Err(e) => {
                    println!("start_background_task error {:?}", e);
                },
                _ => {

                }
            }
        });

        self.start_ping_task();
        self.send_version();

        match self.run(reader).await {
            Ok(_) => {

            },
            Err(err) => {
                println!("peer running error {:?}", err);
                tx_clone.send(PeerMessage::Stop);
            }
        };

        Ok(())
    }

    fn send_version(&self) {
        let my_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0);

        // "bitfield of features to be enabled for this connection"
        let services = constants::ServiceFlags::NONE;

        // "standard UNIX timestamp in seconds"
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time error")
            .as_secs();

        // "The network address of the node receiving this message"
        let addr_recv = address::Address::new(&my_address, constants::ServiceFlags::NONE);

        // "The network address of the node emitting this message"
        let addr_from = address::Address::new(&my_address, constants::ServiceFlags::NONE);

        // "Node random nonce, randomly generated every time a version packet is sent. This nonce is used to detect connections to self."
        let nonce: u64 = 1787878; //secp256k1::rand::thread_rng().gen();

        // "User Agent (0x00 if string is 0 bytes long)"
        let user_agent = String::from("bitcoin");

        // "The last block received by the emitting node"
        let start_height: i32 = 0;
        
        // Construct the message
        let version_message = Message::Version(message_network::VersionMessage::new(
            services,
            timestamp as i64,
            addr_recv,
            addr_from,
            nonce,
            user_agent,
            start_height,
        ));

        self.tx.as_ref().unwrap().send(PeerMessage::Message(version_message));
    }

    async fn start_background_task(mut rx: UnboundedReceiver<PeerMessage>,  mut writer: WriteHalf<TcpStream>) -> Result<(), Box<dyn Error>> {
        while let Some(message) = rx.recv().await {
            match message {
                PeerMessage::Message(msg) => {
                    // println!("got = {:?}", msg);
                    // let mut ss = vec![];
                    // msg.write(&mut ss, magic);

                    let message = RawNetworkMessage {
                        magic: constants::Network::Bitcoin.magic(),
                        payload: msg,
                    };
                    
                    writer.write(encode::serialize(&message).as_slice()).await?;
                },
                PeerMessage::Stop => {
                    println!("stop message");
                    break;
                }
            }
        }

        Ok(())
    }


    fn start_ping_task(&self) {
        let runing = self.runing.clone();
        let tx = self.tx.as_ref().unwrap().clone();
        tokio::spawn(async move {
            loop {
                if runing.load(Ordering::Relaxed) == false {
                    break;
                }
    
                delay_for(Duration::from_millis(1000 * 30)).await;
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                let message = PeerMessage::Message(Message::Ping(now as u64));

                match tx.send(message) {
                    Err(e) => {
                        println!("send message error {:?}", e);
                        runing.store(false, Ordering::Relaxed);
                    },
                    _ => {

                    }
                }
            }
        });
    }

    async fn run(
        &self,
        mut reader: ReadHalf<TcpStream>, 
    ) -> Result<(), Box<dyn Error>> {
        let tx = self.tx.as_ref().unwrap();
        let mut version:Option<Version> = None;

        loop {
            let mut buf = vec![];
            let mut header = [0; 24];

            reader.read_exact(&mut header).await?;

            buf.extend(&header);

            let mut rdr = Cursor::new(&header);

            let mut header = MessageHeader {
                ..Default::default()
            };

            rdr.read(&mut header.magic).await?;
            rdr.read(&mut header.command).await?;
            header.payload_size = AsyncReadBytesExt::read_u32::<LittleEndian>(&mut rdr).await?;
            rdr.read(&mut header.checksum).await?;

            // println!("read header {:?}", header);

            // header.command
            let command = header.get_command();

            let payload_size = header.payload_size as usize;

            let mut buffer = vec![0; payload_size];
            reader.read_exact(buffer.as_mut_slice()).await?;

            buf.extend(&buffer);

            let stream = Cursor::new(&buf);
            let mut stream_reader = StreamReader::new(stream, None);
            let reply: RawNetworkMessage = stream_reader.read_next()?;

            match reply.payload {
                message::NetworkMessage::Tx(tx) => {
                    println!("got a tx {:?}", tx);
                }

                message::NetworkMessage::Block(block) => {
                    println!("got a block {:?}", block.block_hash());
                }

                message::NetworkMessage::Alert(message) => {
                    // println!("alert message is {:?}", std::str::from_utf8(&message));
                }
                
                message::NetworkMessage::Version(version) => {
                    println!("node version is {:?}", version);
                    tx.send(PeerMessage::Message(Message::Verack))?;
                }

                message::NetworkMessage::Verack => {
                    tx.send(PeerMessage::Message(Message::MemPool))?;
                    tx.send(PeerMessage::Message(Message::GetAddr))?;
                }

                message::NetworkMessage::Inv(invs) => {
                    for inv in &invs {
                        match inv { 
                            Inventory::Transaction(tx) => {
                                // println!("got a tx {:?}", tx);
                                
                            },
                            Inventory::Block(block) => {
                                println!("got a block {:?}", block);
                            },
                            _ => {

                            }
                        }
                    }

                    tx.send(PeerMessage::Message(Message::GetData(invs)))?;                    
                }

                message::NetworkMessage::Addr(addrs) => {
                    for addr in addrs {
                        // println!("got a new addr {:?}", addr);
                    }
                }

                message::NetworkMessage::Pong(nonce) => {

                }

                message::NetworkMessage::Ping(ping) => {
                    tx.send(PeerMessage::Message(Message::Pong(ping)))?;
                }
                _ => {
                    println!("command is {}", command);
                }
            }
        }
    }
}