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

mod peer;
use crate::peer::Peer;

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
    // let dir = std::env::current_dir().unwrap();
    // let config_file = dir.join("config.toml");
    
    // let mut f = File::open(config_file).await?;
    // let mut contents = String::new();
    // f.read_to_string(&mut contents).await?;
    // let decoded: AppConfig = toml::from_str(&contents).unwrap();
    // let redis_config = decoded.redis.unwrap();
    
    
    // 

    
    // use std::collections::HashMap;
    // use serde_json::{json, Result, Value};

    // let limit:usize = 1000;
    // let mut id: u64 = 0;
    // let mut file = OpenOptions::new().create(true).append(true).read(true).open("address.txt").await?;
    // let mut contents = String::new();
    // file.read_to_string(&mut contents).await?;
    
    // if contents.len() > 0 {
    //     let list:Vec<&str> = contents.split("\n").collect();

    //     for item in list {
    //         if item.trim().len() > 0 {
    //             let list:Vec<&str> = item.split(":").collect();
    //             addrs.insert(list[0].to_owned());
    //             id = list[1].parse().unwrap();
    //         }
    //     } 
    // }
    

    // loop {
    //     let mut map = HashMap::new();

    //     map.insert("method", "priv.query_address".to_owned());


    //     let params = json!({
    //         "limit": limit,
    //         "id": id,
    //     });

    //     map.insert("params", params.to_string());
    //     println!("map is {:?}", map);

    //     let client = reqwest::Client::new();
    //     let res = client.post("https://volt.id/api.json")
    //         .json(&map)
    //         .send()
    //         .await?
    //         .text()
    //         .await?;
        
    //     let v: Value = serde_json::from_str(&res)?;

    //     match &v["data"] {
    //         Value::Array(list) => {
    //             for item in list {
    //                 let item_id = item["id"].as_u64().unwrap();
    //                 let address = item["address"].as_str().unwrap().to_owned();
    //                 if id < item_id {
    //                     id = item_id;
    //                 }
    //                 let content = format!("{}:{}\n", address, item_id);
    //                 file.write_all(content.as_bytes()).await?;
    //                 addrs.insert(address);
    //             }
    //             if list.len() < limit {
    //                 break;
    //             }
    //         },
    //         _ => {

    //         }
    //     }
    // }

    let seeds = vec![
        "seed.satoshisvision.network",
        "seed.bitprim.org",
        "seed.bitcoinsv.io", 
        "seed.deadalnix.me",   
        "seeder.criptolayer.net",
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
    // tokio::spawn( async move  {
    //     let config = redis_config;
    //     let host = format!("redis://{}:{}/", config.ip.unwrap(), config.port.unwrap());

    //     use redis::{self, AsyncCommands};
    //     loop {
    //         let client = redis::Client::open(host.clone()).expect("can not connect to redis server!");
    //         let mut con = client.get_async_connection().await.unwrap();
    //         println!("connected to redis server");
    //         loop {
    //             match con.brpop::<&str, (String, String)>("threshold_addr_list", 0).await {
    //                 Ok((k, v)) => {
    //                     println!("got a new address {:?}", v);
    //                     let mut lock = addr_map_clone.write().await;
    //                     (*lock).insert(v);
    //                 },
    //                 Err(err) => {
    //                     println!("redis brpop error {:?}", err);
    //                     tokio::time::delay_for(std::time::Duration::from_secs(3)).await;
    //                     break;
    //                 }
    //             }
    //         }
    //     }
    // });


    let (tx,  mut rx) = unbounded_channel::<NodeAddr>();
    let tx_clone = tx.clone();
    
    tokio::spawn(async move {
        for addr in pending_peers {
            tx_clone.send(addr);
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
                        println!("address is {:?}", addr);
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
            let mut peer = Peer::new(nodeAddr.clone(), watch_addrs, tx_clone.clone(), blocks_clone, tx_sender_clone);
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
