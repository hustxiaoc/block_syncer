use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::time::delay_for;
use tokio::time::{Duration};
use tokio::io::{ReadHalf};
use std::error::Error;
use tokio::io::AsyncWriteExt;

#[derive(Debug)]
struct MyError(String);

async fn read(mut reader: &mut ReadHalf<TcpStream>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut buf = [0; 1024];
    reader.read_exact(&mut buf).await?;
    Ok(())
}

async fn connect() -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect("127.0.0.1:9090").await?;
    let (mut reader, mut writer) = tokio::io::split(stream);

    loop {
        tokio::select! {
            _ = delay_for(Duration::from_millis(1000 * 30)) => {
                println!("run delay");
                let mut ss = vec![];
                writer.write(&ss).await?;
            }

            _ = read(&mut reader) => {

            }

            else => {
                
            }
        }
    }

    Ok(())
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {  
    
    let result = tokio::spawn(async move {
        connect().await;
    });

    result.await?;
    Ok(())
}


