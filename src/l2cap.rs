use std::borrow::Borrow;
use crate::smol_fd::SmolFd;
use crate::libc_helpe::libc_check_error;
use libbluetooth::bluetooth::bdaddr_t;
use std::io::{Read, Result, Write};
use std::mem::{size_of, MaybeUninit};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};

type L2CAPSocketAddr = libbluetooth::l2cap::sockaddr_l2;

const SOCKADDR_L2_LEN: usize = size_of::<L2CAPSocketAddr>();

pub struct L2CAPListener {
    fd: SmolFd,
}

impl L2CAPListener {
    pub fn new() -> Result<L2CAPListener> {
        let socket = libc_check_error(unsafe {
            libc::socket(
                libc::AF_BLUETOOTH,
                libc::SOCK_SEQPACKET,
                libbluetooth::bluetooth::BTPROTO_L2CAP,
            )
        })?;

        unsafe {
            let opt: libc::c_int = 0;

            libc_check_error(libc::setsockopt(
                socket,
                libc::SOL_SOCKET,
                libc::SO_SNDBUF,
                &opt as *const i32 as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t
            ))?;

            const BT_POWER: libc::c_int = 9;
            libc_check_error(libc::setsockopt(
                socket,
                libc::SOL_BLUETOOTH,
                BT_POWER,
                &opt as *const i32 as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t
            ))?;
        }

        Ok(L2CAPListener {
            fd: SmolFd::new(socket),
        })
    }

    pub fn bind(&self, psm_port: u16) -> Result<()> {
        let loc_addr = L2CAPSocketAddr {
            l2_family: libbluetooth::bluetooth::AF_BLUETOOTH,
            l2_psm: psm_port,
            l2_bdaddr: libbluetooth::bluetooth::BDADDR_ANY,
            l2_cid: 0,
            l2_bdaddr_type: 0,
        };

        let res = unsafe {
            libc::bind(
                self.fd.raw,
                std::mem::transmute(&loc_addr),
                SOCKADDR_L2_LEN as u32,
            )
        };

        libc_check_error(res)?;
        Ok(())
    }

    pub fn listen(&self, mode: i32) -> Result<()> {
        let res = unsafe { libc::listen(self.fd.raw, mode) };

        libc_check_error(res)?;
        Ok(())
    }

    pub fn accept(&mut self) -> Result<(L2CAPStream, L2CAPSocketAddr)> {
        let mut client_addr: MaybeUninit<L2CAPSocketAddr> = std::mem::MaybeUninit::uninit();
        let mut client_socklen = SOCKADDR_L2_LEN as u32;

        let client = unsafe {
            libc::accept(
                self.fd.raw,
                std::mem::transmute(&mut client_addr),
                &mut client_socklen,
            )
        };

        let client_stream = unsafe { L2CAPStream::from_raw_fd(libc_check_error(client)?) };
        let client_addr: L2CAPSocketAddr = unsafe { std::mem::transmute(client_addr) };

        Ok((client_stream, client_addr))
    }
}

impl Read for L2CAPListener {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.fd.read(buf)
    }
}

impl Write for L2CAPListener {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.fd.write(buf)
    }

    fn flush(&mut self) -> Result<()> {
        self.fd.flush()
    }
}

impl FromRawFd for L2CAPListener {
    unsafe fn from_raw_fd(fd: RawFd) -> L2CAPListener {
        L2CAPListener {
            fd: SmolFd::from_raw_fd(fd),
        }
    }
}

impl AsRawFd for L2CAPListener {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl Drop for L2CAPListener {
    fn drop(&mut self) {
        self.fd.close();
    }
}

pub struct L2CAPStream {
    fd: SmolFd,
}

impl L2CAPStream {
    pub fn new() -> Result<L2CAPStream> {
        let socket = libc_check_error(unsafe {
            libc::socket(
                libc::AF_BLUETOOTH,
                libc::SOCK_SEQPACKET,
                libbluetooth::bluetooth::BTPROTO_L2CAP,
            )
        })?;

        unsafe {
            let opt: libc::c_int = 0;

            libc_check_error(libc::setsockopt(
                socket,
                libc::SOL_SOCKET,
                libc::SO_SNDBUF,
                &opt as *const i32 as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t
            ))?;

            const BT_POWER: libc::c_int = 9;
            libc_check_error(libc::setsockopt(
                socket,
                libc::SOL_BLUETOOTH,
                BT_POWER,
                &opt as *const i32 as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t
            ))?;
        }

        Ok(L2CAPStream {
            fd: SmolFd::new(socket),
        })
    }

    pub fn connect(&mut self, bt_addr: [u8; 6], psm_port: u16) -> Result<()> {
        let loc_addr = L2CAPSocketAddr {
            l2_family: libbluetooth::bluetooth::AF_BLUETOOTH,
            l2_psm: psm_port,
            l2_bdaddr: bdaddr_t { b: bt_addr },
            l2_cid: 0,
            l2_bdaddr_type: 0,
        };

        let res = unsafe {
            libc::connect(
                self.fd.raw,
                std::mem::transmute(&loc_addr),
                SOCKADDR_L2_LEN as u32,
            )
        };

        libc_check_error(res)?;
        Ok(())
    }
}

impl Read for L2CAPStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.fd.read(buf)
    }
}

impl Write for L2CAPStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.fd.write(buf)
    }

    fn flush(&mut self) -> Result<()> {
        self.fd.flush()
    }
}

impl FromRawFd for L2CAPStream {
    unsafe fn from_raw_fd(fd: RawFd) -> L2CAPStream {
        L2CAPStream {
            fd: SmolFd::from_raw_fd(fd),
        }
    }
}

impl AsRawFd for L2CAPStream {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl Drop for L2CAPStream {
    fn drop(&mut self) {
        self.fd.close();
    }
}
