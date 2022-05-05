use libc::EIO;
use std::ffi::c_void;
use std::io::{Result, Error};
// use libbluetooth::hci::{
//     OGF_LINK_POLICY,
//     sniff_mode_cp, OCF_SNIFF_MODE, SNIFF_MODE_CP_SIZE,
//     exit_sniff_mode_cp, OCF_EXIT_SNIFF_MODE, EXIT_SNIFF_MODE_CP_SIZE,
//     evt_mode_change, EVT_MODE_CHANGE, EVT_MODE_CHANGE_SIZE,
// };

use libbluetooth::hci_lib::hci_request;

fn mut_void_ptr<T>(t: &mut T) -> *mut c_void {
    t as *mut T as *mut c_void
}

fn void_ptr<T>(t: &mut T) -> *const c_void {
    t as *const T as *const c_void
}

pub unsafe fn hci_send_req(dd: i32, req: &mut hci_request, timeout: i32) -> std::io::Result<()> {
    let res = libbluetooth::hci_lib::hci_send_req(dd, req, timeout);

    if res < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn hci_sniff_mode(
    dd: i32, handle: u16, max_interval: u16, min_interval: u16,
    sniff_attempt: u16, sniff_timeout: u16, timeout: i32
) -> Result<()> {
    use libbluetooth::hci::{
        OGF_LINK_POLICY,
        sniff_mode_cp, OCF_SNIFF_MODE, SNIFF_MODE_CP_SIZE,
        // evt_mode_change, EVT_MODE_CHANGE, EVT_MODE_CHANGE_SIZE,
        evt_cmd_status, EVT_CMD_STATUS, EVT_CMD_STATUS_SIZE,
    };


    let mut cp = sniff_mode_cp {
        handle: handle,
        max_interval: max_interval,
        min_interval: min_interval,
        attempt: sniff_attempt,
        timeout: sniff_timeout
    };

    // let mut rp = evt_mode_change {
    //     status: 0,
    //     handle: 0,
    //     mode: 0,
    //     interval: 0
    // };

    let mut rp = evt_cmd_status {
        status: 0,
        ncmd: 0,
        opcode: 0
    };

    let mut rq = hci_request {
        ogf: OGF_LINK_POLICY as u16,
        ocf: OCF_SNIFF_MODE as u16,
        // event: EVT_MODE_CHANGE,
        event: EVT_CMD_STATUS,
        cparam: mut_void_ptr(&mut cp),
        clen: SNIFF_MODE_CP_SIZE as i32,
        rparam: mut_void_ptr(&mut rp),
        // rlen: EVT_MODE_CHANGE_SIZE as i32
        rlen: EVT_CMD_STATUS_SIZE as i32
    };

    unsafe {
        if libbluetooth::hci_lib::hci_send_req(dd, &mut rq, timeout) < 0 {
            return Err(Error::last_os_error());
        }
    }

    if rp.status != 0 {
        return Err(Error::from_raw_os_error(EIO));
    }

    Ok(())
}

pub fn hci_exit_sniff_mode(dd: i32, handle: u16, timeout: i32) -> Result<()> {
    use libbluetooth::hci::{
        OGF_LINK_POLICY,
        exit_sniff_mode_cp, OCF_EXIT_SNIFF_MODE, EXIT_SNIFF_MODE_CP_SIZE,
        // evt_mode_change, EVT_MODE_CHANGE, EVT_MODE_CHANGE_SIZE,
        evt_cmd_status, EVT_CMD_STATUS, EVT_CMD_STATUS_SIZE,
    };

    let mut cp = exit_sniff_mode_cp {
        handle: handle,
    };

    // let mut rp = evt_mode_change {
    //     status: 0,
    //     handle: 0,
    //     mode: 0,
    //     interval: 0
    // };

    let mut rp = evt_cmd_status {
        status: 0,
        ncmd: 0,
        opcode: 0
    };

    let mut rq = hci_request {
        ogf: OGF_LINK_POLICY as u16,
        ocf: OCF_EXIT_SNIFF_MODE as u16,
        // event: EVT_MODE_CHANGE,
        event: EVT_CMD_STATUS,
        cparam: mut_void_ptr(&mut cp),
        clen: EXIT_SNIFF_MODE_CP_SIZE as i32,
        rparam: mut_void_ptr(&mut rp),
        // rlen: EVT_MODE_CHANGE_SIZE as i32
        rlen: EVT_CMD_STATUS_SIZE as i32
    };

    unsafe {
        if libbluetooth::hci_lib::hci_send_req(dd, &mut rq, timeout) < 0 {
            return Err(Error::last_os_error());
        }
    }

    if rp.status != 0 {
        return Err(Error::from_raw_os_error(EIO));
    }

    Ok(())
}


