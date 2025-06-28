use bytes::{BufMut, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;

use crate::error::HotStuffError;

pub async fn read_message(reader: &mut ReadHalf<TcpStream>) -> Result<Vec<u8>, HotStuffError> {
    // Read the message length (u32 in network byte order)
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes).await?;
    let len = u32::from_le_bytes(len_bytes) as usize;

    // Read the message body
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;

    Ok(body)
}

pub async fn write_message(
    writer: &mut WriteHalf<TcpStream>,
    message: &[u8],
) -> Result<(), HotStuffError> {
    let len = message.len() as u32;
    let mut buffer = BytesMut::with_capacity(4 + len as usize);
    buffer.put_u32_le(len);
    buffer.put(message);

    writer.write_all(&buffer).await?;
    writer.flush().await?;

    Ok(())
}
