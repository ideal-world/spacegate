#![allow(dead_code)]
use futures_util::future::BoxFuture;
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use spacegate_kernel::service::TcpService;
pub struct Socks5 {}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Socks5Method(u8);

impl Socks5Method {
    pub fn new(method: u8) -> Self {
        Self(method)
    }
    pub fn into_inner(self) -> u8 {
        self.0
    }
    pub const NO_AUTH: Self = Self(0x00);
    pub const GSSAPI: Self = Self(0x01);
    pub const USERNAME_PASSWORD: Self = Self(0x02);
    pub const NO_ACCEPTABLE_METHODS: Self = Self(0xFF);
}

impl From<u8> for Socks5Method {
    fn from(method: u8) -> Self {
        Self::new(method)
    }
}

impl From<Socks5Method> for u8 {
    fn from(val: Socks5Method) -> Self {
        val.into_inner()
    }
}

const CMD_CONNECT: u8 = 0x01;
const CMD_BIND: u8 = 0x02;
const CMD_UDP_ASSOCIATE: u8 = 0x03;
const ATYP_IPV4: u8 = 0x01;
const ATYP_DOMAIN: u8 = 0x03;
const ATYP_IPV6: u8 = 0x04;
const REP_SUCCEEDED: u8 = 0x00;
const REP_GENERAL_FAILURE: u8 = 0x01;
const REP_CONNECTION_NOT_ALLOWED: u8 = 0x02;
const REP_NETWORK_UNREACHABLE: u8 = 0x03;
const REP_HOST_UNREACHABLE: u8 = 0x04;
const REP_CONNECTION_REFUSED: u8 = 0x05;
const REP_TTL_EXPIRED: u8 = 0x06;
const REP_COMMAND_NOT_SUPPORTED: u8 = 0x07;
const REP_ADDRESS_TYPE_NOT_SUPPORTED: u8 = 0x08;

pub struct Socks5Stream<S> {
    bind: std::net::SocketAddr,
    stream: TcpStream,
    state: S,
}

const SOCKS5_VERSION: u8 = 0x05;
pub struct WantHandshake {}

pub struct WantRequest {}

pub struct Finished {}

impl Socks5Stream<WantHandshake> {
    pub async fn handshake(mut self) -> io::Result<Socks5Stream<WantRequest>> {
        let mut buf = [0u8; 2];
        self.stream.read_exact(&mut buf).await?;
        if buf[0] != SOCKS5_VERSION {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid SOCKS version"));
        }
        let methods_count = buf[1] as usize;
        let mut methods = vec![0u8; methods_count];
        self.stream.read_exact(&mut methods).await?;
        if !methods.contains(&Socks5Method::NO_AUTH.into_inner()) {
            self.stream.write_all(&[SOCKS5_VERSION, Socks5Method::NO_ACCEPTABLE_METHODS.into_inner()]).await?;
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "No acceptable methods"));
        }
        self.stream.write_all(&[SOCKS5_VERSION, Socks5Method::NO_AUTH.into_inner()]).await?;

        Ok(Socks5Stream {
            bind: self.bind,
            stream: self.stream,
            state: WantRequest {},
        })
    }
}

impl Socks5Stream<WantRequest> {
    pub async fn relay(mut self) -> io::Result<Socks5Stream<Finished>> {
        let mut buf = [0u8; 4];
        self.stream.read_exact(&mut buf).await?;
        let [ver, cmd, _, atyp] = buf;
        if ver != SOCKS5_VERSION {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid SOCKS version"));
        }
        if cmd != CMD_CONNECT {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Expect CONNECT command"));
        }

        let remote = match atyp {
            ATYP_IPV4 => {
                // IPv4
                let mut addr = [0u8; 4];
                self.stream.read_exact(&mut addr).await?;
                let port = self.stream.read_u16().await?;
                Ok(std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::from(addr)), port))
            }
            ATYP_DOMAIN => {
                // Domain name
                let len = self.stream.read_u8().await? as usize;
                let mut domain = vec![0u8; len];
                self.stream.read_exact(&mut domain).await?;
                let port = self.stream.read_u16().await?;
                let domain = String::from_utf8(domain).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid domain name"))?;
                tokio::net::lookup_host((domain, port)).await?.next().ok_or(REP_HOST_UNREACHABLE)
            }
            ATYP_IPV6 => {
                // IPv6
                let mut addr = [0u8; 16];
                self.stream.read_exact(&mut addr).await?;
                let port = self.stream.read_u16().await?;
                Ok(std::net::SocketAddr::new(std::net::IpAddr::V6(std::net::Ipv6Addr::from(addr)), port))
            }
            _ => Err(REP_ADDRESS_TYPE_NOT_SUPPORTED),
        };
        if let Ok(addr) = remote {
            match TcpStream::connect(addr).await {
                Ok(mut target_stream) => {
                    match self.bind {
                        std::net::SocketAddr::V4(bind_addr) => {
                            self.stream.write_all(&[SOCKS5_VERSION, REP_SUCCEEDED, 0x00, ATYP_IPV4]).await?;
                            self.stream.write_all(&bind_addr.ip().octets()).await?;
                            self.stream.write_u16(addr.port()).await?;
                        }
                        std::net::SocketAddr::V6(bind_addr) => {
                            self.stream.write_all(&[SOCKS5_VERSION, REP_SUCCEEDED, 0x00, ATYP_IPV6]).await?;
                            self.stream.write_all(&bind_addr.ip().octets()).await?;
                            self.stream.write_u16(addr.port()).await?;
                        }
                    }
                    tokio::io::copy_bidirectional(&mut self.stream, &mut target_stream).await?;
                    Ok(Socks5Stream {
                        bind: self.bind,
                        stream: self.stream,
                        state: Finished {},
                    })
                }
                Err(e) => {
                    self.stream.write_all(&[SOCKS5_VERSION, REP_HOST_UNREACHABLE]).await?;
                    Err(e)
                }
            }
        } else {
            self.stream.write_all(&[SOCKS5_VERSION, REP_HOST_UNREACHABLE]).await?;
            Err(io::Error::new(io::ErrorKind::AddrNotAvailable, "Host unreachable"))
        }
    }
}

impl TcpService for Socks5 {
    fn protocol_name(&self) -> &str {
        "socks5"
    }
    fn sniff(&self, peek_buf: &[u8]) -> bool {
        peek_buf.starts_with(b"\x05")
    }
    fn sniff_peek_size(&self) -> usize {
        1
    }
    fn handle(&self, stream: TcpStream, peer: std::net::SocketAddr) -> BoxFuture<'static, spacegate_kernel::BoxResult<()>> {
        Box::pin(async move {
            let bind = peer;
            let handshake = Socks5Stream {
                bind,
                stream,
                state: WantHandshake {},
            };
            let request = handshake.handshake().await?;
            request.relay().await?;
            Ok(())
        })
    }
}
