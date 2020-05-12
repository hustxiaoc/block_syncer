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

use sv::script::Script;
use sv::util::{Hash256, Amount};
use sv::network::Network;
use std::io::Cursor;
use tokio::sync::{ Mutex, Semaphore};

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

mod peer;


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


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {   
    

    // let mut f = File::open("/Users/taoxiaojie/Library/Application Support/Bitcoin/blocks/blk00027.dat").await?;

    // loop {
    //     let magic = AsyncReadBytesExt::read_u32::<LittleEndian>(&mut f).await?;
    //     if magic != 0xd9b4bef9 {
    //         println!("magic = {}", magic);
    //         break;
    //     }

    //     let block_length = AsyncReadBytesExt::read_u32::<LittleEndian>(&mut f).await?;

    //     let mut header = [0; 80];
    //     f.read_exact(&mut header).await;

    //     let mut rdr = Cursor::new(&header[..]);

    //     let block_header = BlockHeader::from_buf(&header).unwrap();
    //     println!("block_header = {:?}", block_header);


    //     let tx_count = read_varint_num!(&mut f);
        
    //     for i in 0..tx_count {
    //         let version = AsyncReadBytesExt::read_u32::<LittleEndian>(&mut f).await?;

    //         let txin_count = read_varint_num!(&mut f);
    //         // println!("txin_count = {:?}", txin_count);

    //         let mut tx_in = vec![];

    //         for i in 0..txin_count {
    //             let mut prev_txid = [0; 32];
    //             f.read_exact(&mut prev_txid).await?;

    //             let output_index = AsyncReadBytesExt::read_u32::<LittleEndian>(&mut f).await?;

    //             let len = read_varint_num!(&mut f);

    //             let mut buffer = vec![0; len as usize];
    //             f.read_exact(buffer.as_mut_slice()).await?;

    //             let sequence_number = AsyncReadBytesExt::read_u32::<LittleEndian>(&mut f).await?;

    //             tx_in.push(
    //                 TxIn {
    //                     prev_output: OutPoint {
    //                         hash: Hash256(prev_txid),
    //                         index: output_index,
    //                     },
    //                     sig_script: Script(buffer),
    //                     sequence: sequence_number,
    //                 }
    //             )
    //         }

    //         let txout_count = read_varint_num!(&mut f);
    //         // println!("txout_count = {:?}", txout_count);

    //         let mut tx_out = vec![];

    //         for j in 0..txout_count {
    //             let satoshis = AsyncReadBytesExt::read_u64::<LittleEndian>(&mut f).await?;

    //             let len = read_varint_num!(&mut f);
    //             let mut script = Script::new();

    //             if len > 0 {
    //                 let mut buffer = vec![0; len as usize];
    //                 f.read_exact(buffer.as_mut_slice()).await?;
    //                 script = Script(buffer);
    //             }

    //             tx_out.push(TxOut{
    //                 amount: Amount(satoshis as i64),
    //                 pk_script: script,
    //             });
    //         }

    //         let lock_time = AsyncReadBytesExt::read_u32::<LittleEndian>(&mut f).await?;
    //         let tx = Tx {
    //             version,
    //             inputs: tx_in,
    //             outputs: tx_out,
    //             lock_time: lock_time,
    //         };

    //         println!("tx = {:?}", tx);
    //     }
    
        
    //     // println!("tx_count = {:?}", tx_count);
    // }
       
    //
    let dir = std::env::current_dir().unwrap();
    let file_path = dir.join("address.txt");
    let config_file = dir.join("config.toml");
    
    {
        let mut f = File::open(config_file).await?;
        let mut contents = String::new();
        f.read_to_string(&mut contents).await?;
        let decoded: Config = toml::from_str(&contents).unwrap();
        // println!("{:#?}", decoded);
    }
    
    let mut f = File::open(file_path).await?;
    
    let mut contents = String::new();
    f.read_to_string(&mut contents).await?;

    println!("current dir = {:?}", dir.to_str().unwrap());

    let list:Vec<&str> = contents.split("\n").collect();
    let mut addrs = HashSet::new();
    for item in list {
        if item.trim().len() > 0 {
            addrs.insert(item.to_owned());
        }
    }

    let seeds = vec![
        "seed.bitcoinsv.io", 
        "seed.bitprim.org",
        "seed.deadalnix.me",   
        "seeder.criptolayer.net",
    ];

    let mut ips: Vec<std::net::IpAddr> = vec![];

    for hostname in seeds {
        match lookup_host(hostname) {
            Ok(list) => {
                ips = list; 
                break;
            },
            Err(err) => {
                println!("lookup_host {} occurs error {:?}", hostname, err);
            }
        };
    }

    let addr_map = Arc::new(Mutex::new(addrs));

    let addr_map_clone = addr_map.clone();
    tokio::spawn( async move  {
        use redis::{self, AsyncCommands};
        loop {
            let client = redis::Client::open("redis://127.0.0.1:6379/").expect("can not connect to redis server!");
            let mut con = client.get_async_connection().await.unwrap();
            println!("connected to redis server");
            loop {
                match con.brpop::<&str, (String, String)>("list_1", 0).await {
                    Ok((k, v)) => {
                        println!("value is {:?}", v);
                        let mut lock = addr_map_clone.lock().await;
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

    for ip in ips {
        let addr = std::net::SocketAddr::new(ip, 8333);
        println!("ips = {:?}", addr);
        let addrs = addr_map.clone();
        tokio::spawn(async move {
            // match peer::Peer::connect(addr, addrs).await {
            //     Err(err) => {
            //         println!("connect {} occurs error {:?}", addr, err);
            //     },
            //     _ => {

            //     }
            // }            
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
