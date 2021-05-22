use tokio::net::TcpStream;
// use tokio::prelude::*;
use tokio::sync::mpsc;
use tokio::time::delay_for;
use tokio::time::{timeout, Duration};

// use byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt};
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};
use std::net::Ipv6Addr;
use sv::messages::{
    Ping,
    BlockHeader,
    OutPoint,
    TxIn, TxOut,
    Tx,
    Version, NodeAddr, Message, MessageHeader
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender, UnboundedReceiver};

use sv::script::Script;
use sv::util::{Hash256, Amount, hash160};
use sv::network::Network;
use sv::address::{addr_encode, AddressType};
use sv::transaction::p2pkh::{extract_pubkeyhash, extract_pubkey};
use cryptoxide::{
    digest::Digest,
    sha2::Sha256
};

use std::io::Cursor;
use tokio::sync::{ RwLock, Mutex, Semaphore};

use std::sync;
use std::sync::{Arc};
use std::thread;
use tokio::fs::{File, OpenOptions};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio_byteorder::{LittleEndian, AsyncWriteBytesExt, AsyncReadBytesExt};
use std::collections::HashSet;
use dns_lookup::{lookup_host, lookup_addr};
use serde_derive::Deserialize;
use toml;
use reqwest;
use std::sync::atomic::{AtomicUsize, Ordering};

use cryptolib::BTCPeer;

use log::{debug, error, info, trace, warn, LevelFilter, SetLoggerError};
use log4rs::{
    append::{
        console::{ConsoleAppender, Target},
        file::FileAppender,
    },
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
    filter::threshold::ThresholdFilter,
};
use lru::LruCache;

// mod peer;


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


#[derive(Debug, Deserialize)]
struct AppConfig {
    global_string: Option<String>,
    global_integer: Option<u64>,
    redis: Option<RedisConfig>,
}

#[derive(Debug, Deserialize)]
struct RedisConfig {
    ip: Option<String>,
    port: Option<u64>,
}


struct PeerManager {

}

impl PeerManager {
    pub fn new() -> Self {
        Self{

        }
    }
    
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {  
    let mut addrs = HashSet::new(); 
    
    let seeds = vec![
        "seed.bitcoin.sipa.be", // Pieter Wuille
        "dnsseed.bluematt.me", // Matt Corallo
        "dnsseed.bitcoin.dashjr.org", // Luke Dashjr
        "seed.bitcoinstats.com", // Christian Decker
        "seed.bitcoin.jonasschnelli.ch", // Jonas Schnelli
        "seed.btc.petertodd.org", // Peter Todd
        "seed.bitcoin.sprovoost.nl", // Sjors Provoost
        "dnsseed.emzy.de" // Stephan Oeste 
    ];
    

    let mut pending_peers: Vec<NodeAddr> = vec![];

    for hostname in seeds {
        match lookup_host(hostname) {
            Ok(list) => {
                for item in list {
                    pending_peers.push(NodeAddr::new(item, 8333));
                }
            },
            Err(err) => {
                println!("lookup_host {} occurs error {:?}", hostname, err);
            }
        };
    }

    let addr_map = Arc::new(RwLock::new(addrs));

    let addr_map_clone = addr_map.clone();
    
    let (tx,  mut rx) = unbounded_channel::<NodeAddr>();
    let tx_clone = tx.clone();
    
    tokio::spawn(async move {
        for addr in pending_peers {
            tx_clone.send(addr);
            // return;
        }
    });

    let connected = Arc::new(AtomicUsize::new(0));
    let mut peers: Arc<Mutex<HashSet<NodeAddr>>> = Arc::new(Mutex::new(HashSet::new()));
    let mut blocks: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

    let (tx_sender,  mut tx_rx) = unbounded_channel::<(Tx, String)>();

    tokio::spawn(async move {
        let mut cache = LruCache::new(1024 * 1024);
        let mut file = OpenOptions::new().create(true).append(true).read(true).open("transaction.txt").await.unwrap();
        
        while let Some((tx, addr)) = tx_rx.recv().await {
            use std::io;
            use std::io::{Read, Write};
            use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
            let hash = tx.hash();

            if cache.contains(&hash) {
                continue;
            }

            cache.put(hash, true);
            println!("got a transaction {:?}", tx.hash());

            let outputs = &tx.outputs;
            let inputs = &tx.inputs;
            for input in inputs {
                match extract_pubkey(&input.sig_script.0) {
                    Ok(ref pubkey) =>  {
                        let pubkeyhash = hash160(pubkey);
                        let addr = addr_encode(&pubkeyhash, AddressType::P2PKH, Network::Mainnet);
                        // println!("address is {:?}", addr);
                    },
                    _ => {}
                }
            }

            for output in outputs {
                let amount = output.amount;

                if amount.0 == 0 {
                    continue;
                }

                match extract_pubkeyhash(&output.pk_script.0) {
                    Ok(ref hash160) => {
                        let address = addr_encode(hash160, AddressType::P2PKH, Network::Mainnet);
                        // let lock = self.watch_addrs.read().await;
                        // if (*lock).contains(&address) {
                        //     self.tx_sender.send((tx.clone(), self.addr.ip.to_string()));
                        // }
                        // println!("tx = {:?}, address = {}, amount = {:?}", tx.hash(), address, amount);
                    },
                    _ => {}
                }
            }
            
            // file.write_all(tx.encode().as_bytes()).await.unwrap();
            // file.write_all(b"\n").await.unwrap();
        }
    });

    while let Some(nodeAddr) = rx.recv().await {
        if connected.load(Ordering::SeqCst) > 50 {
            let mut lock = peers.lock().await;
            (*lock).insert(nodeAddr);
            continue;
        }

        let watch_addrs = addr_map.clone();
        let tx_clone = tx.clone();
        let connected_clone = connected.clone();
        let peers_clone = peers.clone();
        let blocks_clone = blocks.clone();
        let tx_sender_clone = tx_sender.clone();

        tokio::spawn(async move {
            let mut peer = BTCPeer::new(nodeAddr.clone(), watch_addrs, tx_clone.clone(), blocks_clone, tx_sender_clone);
            connected_clone.fetch_add(1, Ordering::SeqCst);

            peer.connect().await;
            connected_clone.fetch_sub(1, Ordering::SeqCst);
            let mut lock = peers_clone.lock().await;
            println!("connect {:?} error", nodeAddr);
            
            while connected_clone.load(Ordering::SeqCst) < 50 {
                if let Some(ele) = (*lock).iter().next().cloned() {
                    (*lock).take(&ele).unwrap();
                    println!("reconnect to {:?}", ele.ip);
                    tx_clone.send(ele);
                }  else {
                    break;
                }
            }
        });
    }
    
    Ok(())
}
