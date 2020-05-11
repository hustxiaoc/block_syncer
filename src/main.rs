use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::sync::mpsc;
use tokio::time::delay_for;
use tokio::time::{timeout, Duration};

use byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt};
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};
use std::net::Ipv6Addr;
use sv::messages::{
    Ping,
    Version, NodeAddr, Message, MessageHeader
};
use sv::network::Network;
use std::io::Cursor;
use tokio::sync::{ Mutex, Semaphore};

use std::sync;
use std::sync::{Arc};
use std::thread;

mod peer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let sem = Arc::new(Semaphore::new(100));
    println!("sem available_permits: {:?}", sem.available_permits());

    for i in 0..30 {
        let ss = sem.clone();
        tokio::spawn(async move {
            println!("start, i = {}", i);
            let xx = ss.acquire().await;
            println!("end, i = {}", i);
            // ss.add_permits(1);
        });
    }

    // let xx = sem.acquire().await;
    // println!("xx = {:?}", xx);
    // sem.add_permits(1);
    // let data = Arc::new(Mutex::new(0u32));
    //
    // let (tx, rx) = sync::mpsc::channel();
    //
    // for _ in 0..10 {
    //     let (data, tx) = (data.clone(), tx.clone());
    //
    //     thread::spawn( async move {
    //         let mut data = data.lock().await;
    //         *data += 1;
    //
    //         tx.send(());
    //     });
    // }
    //
    // for _ in 0..10 {
    //     rx.recv();
    // }
    // Connect to a peer
    // peer::Peer::connect("178.62.228.194:8333").await?;
    //
    // peer::Peer::connect("106.14.176.25:8333").await?;
    //
    loop {
        delay_for(Duration::from_millis(1000 * 30)).await;
        println!("delay runing");
    }
    Ok(())
}
