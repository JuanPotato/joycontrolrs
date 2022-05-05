use std::ffi::c_void;
use std::os::raw::{c_char, c_int};
use std::io::Result;
use libbluetooth::bluetooth::SOL_HCI;
use libbluetooth::hci::{HCI_EVENT_PKT, hci_filter, HCI_FILTER};
use crate::libc_helpe::libc_check_error;
use libbluetooth::hci_lib::{self, hci_open_dev, hci_close_dev, hci_read_class_of_dev, hci_write_class_of_dev, hci_filter_clear_opcode};
use libc::setsockopt;

pub struct HciDev {
    hdev: c_int,
    socket: c_int,
}

impl HciDev {
    pub unsafe fn new(hdev: c_int) -> Result<HciDev> {
        let socket = hci_open_dev(hdev);

        Ok(HciDev {
            hdev: hdev,
            socket: libc_check_error(socket)?,
        })
    }

    pub unsafe fn write_class(&mut self, cod: u32) -> Result<()> {
        let write_res = hci_write_class_of_dev(self.socket, cod, 2000);
        libc_check_error(write_res)?;
        Ok(())
    }

    pub unsafe fn read_class(&mut self) -> Result<u32> {
        let mut class = [0u8; 4];
        let read_res = hci_read_class_of_dev(self.socket, class[1..].as_mut_ptr() as *mut c_char, 2000);
        libc_check_error(read_res)?;

        Ok(u32::from_be_bytes(class))
    }

    pub unsafe fn cmd_cmd(&mut self, ocf: u16, ogf: u16, cmd: &mut [u8]) -> std::io::Result<Vec<u8>> {
        let mut flt = hci_filter {
            type_mask: 0,
            event_mask: [0, 0],
            opcode: 0
        };

        /* Setup filter */
        hci_lib::hci_filter_set_ptype(HCI_EVENT_PKT, &mut flt);
        hci_lib::hci_filter_all_events(&mut flt);

        libc_check_error(setsockopt(
            self.socket,
            SOL_HCI,
            HCI_FILTER,
            &flt as *const hci_filter as *const c_void,
            std::mem::size_of::<hci_filter>() as libc::socklen_t
        ))?;

        libc_check_error(hci_lib::hci_send_cmd(
            self.socket,
            ogf,
            ocf,
            cmd.len() as c_char,
            cmd.as_mut_ptr() as *mut c_void
        ))?;

        let mut buf = vec![0u8; libbluetooth::hci::HCI_MAX_EVENT_SIZE];
        let len = libc::read(self.socket, buf.as_mut_ptr() as *mut c_void, buf.len());
        libc_check_error(len)?;

        buf.resize(len as usize, 0);
        Ok(buf)
    }
    // pub unsafe fn write_cmd(&mut self) -> Result<()> {
    //     hci_write_
    // }
}

impl Drop for HciDev {
    fn drop(&mut self) {
        unsafe {
            hci_close_dev(self.socket);
        }
    }
}
