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
use sv::util::{Hash256, Amount};
use sv::network::Network;
use std::io::Cursor;
use tokio::sync::{ RwLock, Mutex, Semaphore};

use std::sync;
use std::sync::{Arc};
use std::thread;
use tokio::fs::File;
use tokio::io::{self, AsyncReadExt};
use tokio_byteorder::{LittleEndian, AsyncWriteBytesExt, AsyncReadBytesExt};
use std::collections::HashSet;
use dns_lookup::{lookup_host, lookup_addr};
use serde_derive::Deserialize;
use toml;
use reqwest;
use std::sync::atomic::{AtomicUsize, Ordering};

mod peer;
use crate::peer::Peer;
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
struct Config {
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
    
    let dir = std::env::current_dir().unwrap();
    let file_path = dir.join("address.txt");
    let config_file = dir.join("config.toml");
    
    let mut f = File::open(config_file).await?;
    let mut contents = String::new();
    f.read_to_string(&mut contents).await?;
    let decoded: Config = toml::from_str(&contents).unwrap();
    let redis_config = decoded.redis.unwrap();
    
    
    let mut addrs = HashSet::new();

    let mut f = File::open(file_path).await?;
    
    let mut contents = String::new();
    f.read_to_string(&mut contents).await?;
    
    let list:Vec<&str> = contents.split("\n").collect();

    for item in list {
        if item.trim().len() > 0 {
            addrs.insert(item.to_owned());
        }
    }

    use std::collections::HashMap;
    use serde_json::{json, Result, Value};

    let limit:usize = 100;
    let mut page:usize =  1;

    // loop {
    //     let mut map = HashMap::new();

    //     map.insert("method", "priv.query_address".to_owned());


    //     let params = json!({
    //         "limit": limit,
    //         "page": page,
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
    //             page = page + 1;
    //             for item in list {
    //                 addrs.insert(item["address"].as_str().unwrap().to_owned());
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
    tokio::spawn( async move  {
        let config = redis_config;
        let host = format!("redis://{}:{}/", config.ip.unwrap(), config.port.unwrap());

        use redis::{self, AsyncCommands};
        loop {
            let client = redis::Client::open(host.clone()).expect("can not connect to redis server!");
            let mut con = client.get_async_connection().await.unwrap();
            println!("connected to redis server");
            loop {
                match con.brpop::<&str, (String, String)>("list_1", 0).await {
                    Ok((k, v)) => {
                        println!("value is {:?}", v);
                        let mut lock = addr_map_clone.write().await;
                        (*lock).insert(v);
                    },
                    Err(err) => {
                        println!("redis brpop error {:?}", err);
                        tokio::time::delay_for(std::time::Duration::from_secs(3)).await;
                        break;
                    }
                }
            }
        }
    });


    let (tx,  mut rx) = unbounded_channel::<NodeAddr>();
    

    let tx_clone = tx.clone();
    tokio::spawn(async move {
        for addr in pending_peers {
            tx_clone.send(addr);
        }
    });

    // for ip in ips {
    //     let addr = std::net::SocketAddr::new(ip, 8333);
    //     println!("ip = {:?}", addr);
    //     let addrs = addr_map.clone();
    //     let tx_clone = tx.clone();
    //     let connected = connected.clone();
    //     tokio::spawn(async move {
    //         let mut peer = Peer::new(addr.to_string(), addrs, tx_clone);
    //         match peer.connect().await {
    //             Err(err) => {
    //                 println!("connect {} occurs error {:?}", addr, err);
    //             },
    //             _ => {
    //                 connected.fetch_add(1, Ordering::SeqCst);
    //             }
    //         }           
    //     });
    // }

    let connected = Arc::new(AtomicUsize::new(0));
    let mut peers: Arc<Mutex<HashSet<NodeAddr>>> = Arc::new(Mutex::new(HashSet::new()));
    let mut blocks: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

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

        tokio::spawn(async move {
            let ip = nodeAddr.ip;
            let mut peer = Peer::new(nodeAddr, watch_addrs, tx_clone.clone(), blocks_clone);
            connected_clone.fetch_add(1, Ordering::SeqCst);

            peer.connect().await;
            connected_clone.fetch_sub(1, Ordering::SeqCst);
            let mut lock = peers_clone.lock().await;
            println!("connect {:?} error", ip);
            
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
    
   
    // peer::Peer::connect("178.62.228.194:8333", addrs).await?;

    // peer::Peer::connect("178.128.147.160:8333", addrs).await?;
    //

    loop {
        delay_for(Duration::from_millis(1000 * 30)).await;
        println!("delay runing");
    }
    Ok(())
}
