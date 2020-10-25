//! UDP framing

const INITIAL_RD_CAPACITY: usize = 64 * 1024;
const INITIAL_WR_CAPACITY: usize = 8 * 1024;

mod frame;
mod framed_recv;
mod framed_send;
pub use frame::UdpFramed;
pub use framed_recv::UdpFramedRecv;
pub use framed_send::UdpFramedSend;
