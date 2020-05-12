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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {     
    let mut f = File::open("/Users/taoxiaojie/Library/Application Support/Bitcoin/blocks/blk00027.dat").await?;

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
       
    // peer::Peer::connect("178.62.228.194:8333").await?;
    //
    peer::Peer::connect("178.128.147.160:8333").await?;
    //

    let xx = "block";
    let mut b = *b"block\0\0\0\0\0\0";
    println!("command = |{:?}|", b.to_vec());

    let new = b.to_vec().iter().filter(|&c| {
        *c > 0
    }).map(|x| *x).collect::<Vec<u8>>();

    let s = std::str::from_utf8(new.as_ref()).unwrap().trim();

    println!("command = |{:?}|", s.as_bytes());
    println!("block = |{:?}|", xx.as_bytes());

    println!("s == block ? {}", s == "block");
    loop {
        delay_for(Duration::from_millis(1000 * 30)).await;
        println!("delay runing");
    }
    Ok(())
}
