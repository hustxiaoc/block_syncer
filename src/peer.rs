use tokio::net::TcpStream;
use tokio::prelude::*;
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

    pub async fn connect<A: tokio::net::ToSocketAddrs>(addr: A, addrs: HashSet<String>) -> Result<Peer, Box<dyn Error>> {
        let dir = std::env::current_dir().unwrap();
        println!("current dir = {:?}", dir.to_str().unwrap());

        let mut stream = TcpStream::connect(addr).await?;
        let (mut reader, mut writer) = tokio::io::split(stream);
        let (mut tx, mut rx) = unbounded_channel::<PeerMessage>();

        let peer = Peer {
            addr: "addr".to_owned(),
            runing: Arc::new(AtomicBool::new(true)),
        };

        tokio::spawn(Self::start_background_task(
            rx,
            writer,
        ));

        tokio::spawn(Self::start_ping_task(peer.runing.clone(), tx.clone()));

        let running = peer.runing.clone();
        tokio::spawn(async move {
            match Self::run(running, tx, reader, addrs).await {
                Ok(_) => {},
                Err(err) => {
                    println!("peer running error {:?}", err);
                }
            }
        });

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

    async fn run(runing: Arc<AtomicBool>, tx: UnboundedSender<PeerMessage>, mut reader: ReadHalf<TcpStream>, addrs: HashSet<String>) -> Result<(), Box<dyn Error>>  {

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
            user_agent: "/Bitcoin SV:1.0.2/".into(),
            start_height: 0,
            relay: true,
        };

        tx.send(PeerMessage::Message(Message::Version(version)));

        loop {
            let mut header = [0; 24];

            let n = reader.read_exact(&mut header).await?;

            let mut rdr = Cursor::new(&header);

            let mut header = MessageHeader {
                ..Default::default()
            };

            rdr.read(&mut header.magic).await;
            rdr.read(&mut header.command).await;
            header.payload_size = AsyncReadBytesExt::read_u32::<LittleEndian>(&mut rdr).await?;
            rdr.read(&mut header.checksum).await;

            // println!("read header {:?}", header);

            // header.command
            let command = header.get_command();
            // println!("command is {}", command);

            let payload_size = header.payload_size as usize;


            if command == "block" {
                println!("start reading block");
                let mut header = [0; 80];
                reader.read_exact(&mut header).await;

                let mut rdr = Cursor::new(&header[..]);

                let block_header = BlockHeader::from_buf(&header).unwrap();
                println!("[begin] block header = {:?}", block_header);


                let tx_count = read_varint_num!(&mut reader);
                
                for i in 0..tx_count {
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

                    for j in 0..txout_count {
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

                    let outputs = &tx.outputs;
                    for output in outputs {
                        let amount = output.amount;

                        if amount.0 == 0 {
                            continue;
                        }

                        match extract_pubkeyhash(&output.pk_script.0) {
                            Ok(ref hash160) => {
                                let address = addr_encode(hash160, AddressType::P2PKH, Network::Mainnet);
                                if addrs.contains(&address) {
                                    println!("tx = {:?}, address = {}, amount = {:?}", tx.hash(), address, amount);
                                }
                                println!("tx = {:?}, address = {}, amount = {:?}", tx.hash(), address, amount);
                            },
                            Err(err) => {

                            }
                        }
                    }
                }
                println!("[end] block header = {:?}", block_header);
                continue;
            }

            let mut buffer = vec![0; payload_size];
            reader.read_exact(buffer.as_mut_slice()).await?;

            let mut rdr = Cursor::new(&buffer);
            let message = Message::read_partial(&mut rdr, &header);
            // println!("{:?}", message);

            match message {
                Ok(Message::Tx(ref tx)) => {
                    // println!("transaction {:?}", tx.hash());
                    let outputs = &tx.outputs;
                    for output in outputs {
                        let amount = output.amount;

                        if amount.0 == 0 {
                            continue;
                        }

                        match extract_pubkeyhash(&output.pk_script.0) {
                            Ok(ref hash160) => {
                                let address = addr_encode(hash160, AddressType::P2PKH, Network::Mainnet);
                                if addrs.contains(&address) {
                                    println!("tx = {:?}, address = {}, amount = {:?}", tx.hash(), address, amount);
                                }
                                println!("tx = {:?}, address = {}, amount = {:?}", tx.hash(), address, amount);
                            },
                            Err(err) => {

                            }
                        }
                    }
                },

                Ok(Message::Version(v)) => {
                    println!("version {:?}", v);
                    tx.send(PeerMessage::Message(Message::Verack));


                    let mut inv = Inv {
                        objects: Vec::new(),
                    };

                    inv.objects.push(InvVect {
                        obj_type: INV_VECT_BLOCK,
                        hash: Hash256::decode("00000000000000000431fe7834b59fd4ff5a296db88944f4a8f2e3e6761465d6").unwrap(),
                    });

                    // tx.send(PeerMessage::Message(Message::GetData(inv)));

                    // let locator = BlockLocator {
                    //     version: 70015,
                    //     block_locator_hashes: vec![   
                    //         NO_HASH_STOP,
                    //         Hash256::decode("6677889900667788990066778899006677889900667788990066778899006677")
                    //             .unwrap(),
                    //     ],
                    //     hash_stop: Hash256::decode(
                    //         "1122334455112233445511223344551122334455112233445511223344551122",
                    //     )
                    //     .unwrap(),
                    // };

                    // tx.send(PeerMessage::Message(Message::GetHeaders(locator)));
                },

                Ok(Message::Block(block)) => {
                    println!("got a block {:?}", block);
                },

                Ok(Message::Headers(headers)) => {
                    let headers = headers.headers;
                    for header in headers {
                        println!("header = {:?}", header.hash());
                        let mut inv = Inv {
                            objects: Vec::new(),
                        };

                        inv.objects.push(InvVect {
                            obj_type: INV_VECT_BLOCK,
                            hash: header.hash(),
                        });

                        // tx.send(PeerMessage::Message(Message::GetBlocks));
                        // break;
                    }
                    
                },

                Ok(Message::Verack) => {
                    // request Mempool transactions
                    // tx.send(PeerMessage::Message(Message::Mempool));
                    tx.send(PeerMessage::Message(Message::GetAddr));
                },

                Ok(Message::Inv(inv)) => {
                    tx.send(PeerMessage::Message(Message::GetData(inv)));
                },

                Ok(Message::Ping(ping)) => {
                    tx.send(PeerMessage::Message(Message::Pong(ping)));
                },

                Ok(Message::Addr(addr)) => {
                    println!("get a addr message {:?}", addr);
                    let addrs = addr.addrs;
                    for address in addrs {
                        println!("get a address message {:?}", address);
                    }
                }

                _ => {
                    println!("get a message {:?}", message);
                }
            };
        }
    }
}
