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
    Version, NodeAddr, Message, MessageHeader
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

        self.start_background_task(rx, writer);
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
            user_agent: "/Bitcoin SV:1.0.7.1/".into(),
            start_height: 0,
            relay: true,
        };
        self.tx.as_ref().unwrap().send(PeerMessage::Message(Message::Version(version)));
    }

    fn start_background_task(&self, mut rx: UnboundedReceiver<PeerMessage>,  mut writer: WriteHalf<TcpStream>) {
        tokio::spawn(async move {
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
        });
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
                let message = PeerMessage::Message(Message::Ping(Ping{
                    nonce: now as u64,
                }));

                match tx.send(message) {
                    Err(e) => {
                        println!("send message error {:?}", e);
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
            let mut header = [0; 24];

            reader.read_exact(&mut header).await?;

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
            // println!("command is {}", command);

            let payload_size = header.payload_size as usize;


            if command == "block" {
                println!("start reading block");
                let mut header = [0; 80];
                reader.read_exact(&mut header).await?;

                let block_header = BlockHeader::from_buf(&header).unwrap();
                println!("[begin] block = {:?}", block_header.hash());
                let block_hash = block_header.hash().encode();
                {
                    let lock = self.blocks.lock().await;
                    // 该区块已经处理
                    if (*lock).contains(&block_hash) {
                        println!("drain buf {}", payload_size);
                        drain_buf!(&mut reader, payload_size, 1024);
                        continue;
                    }
                }

                let tx_count = read_varint_num!(&mut reader);
                
                for _ in 0..tx_count {
                    let version = AsyncReadBytesExt::read_u32::<LittleEndian>(&mut reader).await?;

                    let txin_count = read_varint_num!(&mut reader);
                    // println!("txin_count = {:?}", txin_count);

                    let mut tx_in = vec![];

                    for i in 0..txin_count {
                        let mut prev_txid = [0; 32];
                        reader.read_exact(&mut prev_txid).await?;

                        let output_index = AsyncReadBytesExt::read_u32::<LittleEndian>(&mut reader).await?;

                        let len = read_varint_num!(&mut reader);

                        let mut buffer = vec![0; len as usize];
                        reader.read_exact(buffer.as_mut_slice()).await?;

                        let sequence_number = AsyncReadBytesExt::read_u32::<LittleEndian>(&mut reader).await?;

                        tx_in.push(
                            TxIn {
                                prev_output: OutPoint {
                                    hash: Hash256(prev_txid),
                                    index: output_index,
                                },
                                sig_script: Script(buffer),
                                sequence: sequence_number,
                            }
                        )
                    }

                    let txout_count = read_varint_num!(&mut reader);
                    // println!("txout_count = {:?}", txout_count);

                    let mut tx_out = vec![];

                    for _ in 0..txout_count {
                        let satoshis = AsyncReadBytesExt::read_u64::<LittleEndian>(&mut reader).await?;

                        let len = read_varint_num!(&mut reader);
                        let mut script = Script::new();

                        if len > 0 {
                            let mut buffer = vec![0; len as usize];
                            reader.read_exact(buffer.as_mut_slice()).await?;
                            script = Script(buffer);
                        }

                        tx_out.push(TxOut{
                            amount: Amount(satoshis as i64),
                            pk_script: script,
                        });
                    }

                    let lock_time = AsyncReadBytesExt::read_u32::<LittleEndian>(&mut reader).await?;
                    let tx = Tx {
                        version,
                        inputs: tx_in,
                        outputs: tx_out,
                        lock_time: lock_time,
                    };

                    self.tx_sender.send((tx.clone(), self.addr.ip.to_string()));

                    // let outputs = &tx.outputs;
                    // for output in outputs {
                    //     let amount = output.amount;

                    //     if amount.0 == 0 {
                    //         continue;
                    //     }

                    //     match extract_pubkeyhash(&output.pk_script.0) {
                    //         Ok(ref hash160) => {
                    //             let address = addr_encode(hash160, AddressType::P2PKH, Network::Mainnet);
                    //             let lock = self.watch_addrs.read().await;
                    //             if (*lock).contains(&address) {
                    //                 self.tx_sender.send((tx.clone(), self.addr.ip.to_string()));
                    //             }
                    //             // println!("tx = {:?}, address = {}, amount = {:?}", tx.hash(), address, amount);
                    //         },
                    //         _ => {}
                    //     }
                    // }
                }
                println!("[end] block = {:?}", block_header.hash());
                {
                    let mut lock = self.blocks.lock().await;
                    // 该区块已经处理
                    (*lock).insert(block_hash);
                }
                continue;
            }

            let mut buffer = vec![0; payload_size];
            reader.read_exact(buffer.as_mut_slice()).await?;

            let mut rdr = Cursor::new(&buffer);
            let message = Message::read_partial(&mut rdr, &header);
            // println!("{:?}", message);

            match message {
                Ok(Message::Tx(ref tx)) => {
                    // println!("got transaction {:?}", tx.hash());
                    self.tx_sender.send((tx.clone(), self.addr.ip.to_string()));
                },

                Ok(Message::NotFound(inv)) => {
                    // let items = inv.objects;
                    // for item in items {
                    //     // This node cannot find init tx, we disconnect it.
                    //     if item.obj_type == INV_VECT_TX && item.hash.encode() == init_tx {
                    //         let error = "Node cannot find target transaction";
                    //         return Err(error.into());
                    //     }
                    // }
                },

                Ok(Message::Version(v)) => {
                    println!("version {:?}", v);
                    version = Some(v);
                    tx.send(PeerMessage::Message(Message::Verack));


                    // let mut inv = Inv {
                    //     objects: Vec::new(),
                    // };

                    // inv.objects.push(InvVect {
                    //     obj_type: INV_VECT_TX,
                    //     hash: Hash256::decode(init_tx).unwrap(),
                    // });

                    // tx.send(PeerMessage::Message(Message::GetData(inv)));

                    let locator = BlockLocator {
                        version: 70015,
                        block_locator_hashes: vec![   
                            NO_HASH_STOP,
                            Hash256::decode(init_block_header).unwrap(),
                        ],
                        hash_stop: Hash256::decode(init_block_header).unwrap(),
                    };

                    tx.send(PeerMessage::Message(Message::GetHeaders(locator)));
                },

                Ok(Message::Block(block)) => {
                    println!("got a block {:?}", block);
                },

                Ok(Message::GetBlocks(_)) => {
                    let error = format!("We don't provide such service byebye! node with version {:?}", version);
                    return Err(error.into());
                },

                Ok(Message::Headers(headers)) => {
                    let headers = headers.headers;
                    for header in headers {
                        if header.hash().encode() == first_block_header {
                            let error = format!("Invalid node with version {:?}", version);
                            return Err(error.into());
                        }

                        // println!("header = {:?}", header.hash());
                        // let mut inv = Inv {
                        //     objects: Vec::new(),
                        // };

                        // inv.objects.push(InvVect {
                        //     obj_type: INV_VECT_BLOCK,
                        //     hash: header.hash(),
                        // });

                        // tx.send(PeerMessage::Message(Message::GetBlocks));
                        // break;
                    }
                    
                },

                Ok(Message::Verack) => {
                    // request Mempool transactions
                    // tx.send(PeerMessage::Message(Message::Mempool));
                    tx.send(PeerMessage::Message(Message::GetAddr)).unwrap();
                },

                Ok(Message::Inv(inv)) => {
                    tx.send(PeerMessage::Message(Message::GetData(inv))).unwrap();
                },

                Ok(Message::Ping(ping)) => {
                    tx.send(PeerMessage::Message(Message::Pong(ping))).unwrap();
                },

                Ok(Message::Pong(pong)) => {

                },

                Ok(Message::Addr(addr)) => {
                    // println!("get a addr message {:?}", addr);
                    let addrs = addr.addrs;
                    
                    for address in addrs {
                        // no network service
                        if address.addr.services & 1 == 0 {
                            continue;
                        }
                        // println!("get a address message {:?}", address);
                        self.addr_tx.send(address.addr).unwrap();
                    }
                }

                _ => {
                    println!("get a message {:?}", message);
                }
            };
        }
    }
}