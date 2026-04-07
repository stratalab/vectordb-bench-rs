//! Length-prefixed frame protocol.
//!
//! Mirrors `strata-core`'s `crates/executor/src/ipc/wire.rs`:
//!
//! - Each frame = 4-byte big-endian u32 length prefix, then `length` bytes of
//!   MessagePack payload.
//! - Maximum frame size: 64 MB. Any larger frame (encoded or decoded) is an
//!   error on both ends.
//!
//! Kept deliberately minimal: no connection pooling, no pipelining, no retry.
//! The runner issues one request at a time on a single client.

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{IpcClientError, Result};

/// Matches `MAX_FRAME_SIZE` in `strata-core`'s `ipc/wire.rs`.
pub(crate) const MAX_FRAME_SIZE: u32 = 64 * 1024 * 1024;

/// Write a length-prefixed frame to the stream.
pub(crate) async fn write_frame<W: AsyncWriteExt + Unpin>(
    stream: &mut W,
    payload: &[u8],
) -> Result<()> {
    let len = payload.len();
    if len > MAX_FRAME_SIZE as usize {
        return Err(IpcClientError::FrameTooLarge {
            bytes: len,
            max: MAX_FRAME_SIZE as usize,
        });
    }
    let len_bytes = (len as u32).to_be_bytes();
    stream.write_all(&len_bytes).await?;
    stream.write_all(payload).await?;
    stream.flush().await?;
    Ok(())
}

/// Read a length-prefixed frame from the stream.
pub(crate) async fn read_frame<R: AsyncReadExt + Unpin>(stream: &mut R) -> Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf);

    if len > MAX_FRAME_SIZE {
        return Err(IpcClientError::FrameTooLarge {
            bytes: len as usize,
            max: MAX_FRAME_SIZE as usize,
        });
    }

    let mut payload = vec![0u8; len as usize];
    stream.read_exact(&mut payload).await?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn round_trip_frame() {
        let (mut a, mut b) = duplex(1024);
        let payload = b"hello world";
        write_frame(&mut a, payload).await.unwrap();
        let got = read_frame(&mut b).await.unwrap();
        assert_eq!(got, payload);
    }

    #[tokio::test]
    async fn round_trip_empty_frame() {
        let (mut a, mut b) = duplex(1024);
        write_frame(&mut a, b"").await.unwrap();
        let got = read_frame(&mut b).await.unwrap();
        assert!(got.is_empty());
    }

    #[tokio::test]
    async fn oversized_write_rejected() {
        let (mut a, _b) = duplex(1024);
        let payload = vec![0u8; (MAX_FRAME_SIZE as usize) + 1];
        let err = write_frame(&mut a, &payload).await.unwrap_err();
        assert!(matches!(err, IpcClientError::FrameTooLarge { .. }));
    }
}
