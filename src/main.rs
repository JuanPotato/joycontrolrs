use blurz::{BluetoothAdapter, BluetoothDevice, BluetoothDiscoverySession, BluetoothSession};

use std::collections::HashMap;
use std::error::Error;
use std::process::Command;
use std::time::Duration;

mod dbus_profile_manager;
mod l2cap;
mod smol_fd;
mod libc_helpe;
mod hci_utils;
mod hcidev;

use l2cap::{L2CAPListener, L2CAPStream};

use dbus::arg::{RefArg, Variant};
use dbus::blocking::Connection;
use dbus_profile_manager::OrgBluezProfileManager1;
use std::num::ParseIntError;
use std::io::Read;

use smol::Async;
use futures::prelude::*;

const CONTROLLERS: [&str; 3] = ["Pro Controller", "Joy-Con (L)", "Joy-Con (R)"];

macro_rules! insert {
    ($map:ident, $key:expr, $val:expr) => {
        $map.insert($key, Variant(Box::new($val) as Box<dyn RefArg>));
    };
}

fn scan_for_bluetooth_controller<'a>(
    session: &'a BluetoothSession,
    adapter: &'a BluetoothAdapter,
) -> BluetoothDevice<'a> {
    let discovery = BluetoothDiscoverySession::create_session(&session, adapter.get_id()).unwrap();
    discovery.start_discovery().unwrap();

    println!("Will start to scan for controllers.");

    let bt_controller = 'outer_loop: loop {
        let devices = adapter.get_device_list().unwrap();

        'device_loop: for device in devices {
            let bt_device = blurz::bluetooth_device::BluetoothDevice::new(&session, device);

            let id = bt_device.get_id();
            let rssi = bt_device.get_rssi();
            let alias = bt_device.get_alias().unwrap_or(String::new());

            if rssi.is_ok() {
                println!("Found device: '{}' ({})", &alias, &id);
            } else {
                continue 'device_loop;
            }

            if CONTROLLERS.contains(&alias.as_str()) {
                println!("Found {}. Will connect after restart.", &alias);

                discovery.stop_discovery().unwrap();

                break 'outer_loop bt_device;
            }
        }

        std::thread::sleep(Duration::from_secs(5));
    };

    return bt_controller;
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
struct BtAddr(pub [u8; 6]);

impl BtAddr {
    pub fn from_str(addr_str: &str) -> Result<BtAddr, ParseIntError> {
        let mut addr = [0; 6];

        for i in 0..6 {
            addr[i] = u8::from_str_radix(&addr_str[i * 3..i * 3 + 2], 16)?;
        }

        return Ok(BtAddr(addr));
    }

    /// Linux lower-layers actually hold the address in native byte-order
    /// althrough they are always displayed in network byte-order
    #[inline(always)]
    #[cfg(target_endian = "little")]
    pub fn convert_host_byteorder(mut self) -> BtAddr {
        {
            let (value_1, value_2) = (&mut self.0).split_at_mut(3);
            std::mem::swap(&mut value_1[0], &mut value_2[2]);
            std::mem::swap(&mut value_1[1], &mut value_2[1]);
            std::mem::swap(&mut value_1[2], &mut value_2[0]);
        }

        self
    }

    #[inline(always)]
    #[cfg(target_endian = "big")]
    pub fn convert_host_byteorder(self) -> BtAddr {
        // Public address structure contents are always big-endian
        self
    }
}

impl std::fmt::Display for BtAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let id = 2;

    let session = BluetoothSession::create_session(None).unwrap();
    let adapter = BluetoothAdapter::create_adapter(&session, format!("/org/bluez/hci{id}")).unwrap();
    let adapter_addr_str = adapter.get_address().unwrap();
    let adapter_addr = BtAddr::from_str(&adapter_addr_str).unwrap();

    let controller = scan_for_bluetooth_controller(&session, &adapter);
    let controller_addr = controller.get_address().unwrap();
    let controller_name = controller.get_alias().unwrap();

    println!("{}: {}", controller_name, controller_addr);
    let controller_btaddr = BtAddr::from_str(&controller_addr).unwrap();
    let converted_btaddr = controller_btaddr.convert_host_byteorder();

    let mut ctl_server_l2cap = L2CAPListener::new()?;
    let mut itr_server_l2cap = L2CAPListener::new()?;

    println!("Restarting bluetooth service...");

    let mut cmd = Command::new("systemctl");
    cmd.arg("restart");
    cmd.arg("bluetooth.service");
    cmd.spawn().unwrap().wait().unwrap();

    std::thread::sleep(Duration::from_secs(1));

    println!("Connecting to controller.");

    let mut controller_ctl_l2cap = L2CAPStream::new().unwrap();
    let mut controller_itr_l2cap = L2CAPStream::new().unwrap();

    if let Err(e) = controller_ctl_l2cap.connect(converted_btaddr.0, 17) {
        println!("Could not connect to controller");
        return Err(e)?;
    }

    if let Err(e) = controller_itr_l2cap.connect(converted_btaddr.0, 19) {
        println!("Could not connect to controller");
        return Err(e)?;
    }

    println!("Now we flush out all the data we're receiving from the controller until we're ready to use it.");

    let (mut stop_tx, mut stop_rx) = futures::channel::oneshot::channel::<()>();

    let flush_thread = std::thread::spawn(move || {
        let mut itr = controller_itr_l2cap;
        let mut buf = [0u8; 64];

        loop {
            // I'm not using async here because when I did it just refused to read even though
            // it works completely fine in the actual forwarding down below, which is very weird.
            let res = itr.read(&mut buf);

            match stop_rx.try_recv() {
                Ok(Some(())) => return Ok(itr),
                Ok(None) => continue,
                Err(e) => return Err(e),
            }
        }
    });

    println!("Binding server to necessary ports. This will fail if we aren't root.");

    ctl_server_l2cap.bind(17)?;
    itr_server_l2cap.bind(19)?;

    ctl_server_l2cap.listen(1)?;
    itr_server_l2cap.listen(1)?;

    println!("Changing name and class");

    let session = BluetoothSession::create_session(None)?;
    let adapter = BluetoothAdapter::create_adapter(&session, format!("/org/bluez/hci{id}")).unwrap();

    adapter.set_alias(controller_name)?;

    // TODO: Make this not hci0, but any hci you want
    let mut hci_device = unsafe {
        hcidev::HciDev::new(id)
    }.unwrap();

    println!("Advertising the Bluetooth SDP record...");
    println!("Please open the \"Change Grip/Order\" menu.");

    const PROFILE_DATA: &str = include_str!("../sdp_record_hid.xml");
    const HID_UUID: &str = "00001124-0000-1000-8000-00805f9b34fb";
    const HID_PATH: &str = "/bluez/switch/hid";

    let conn = Connection::new_system()?;
    let proxy = conn.with_proxy("org.bluez", "/org/bluez", Duration::from_millis(5000));

    let mut options = HashMap::new();
    insert!(options, "ServiceRecord", PROFILE_DATA.to_string());
    insert!(options, "Role", "server".to_string());
    insert!(options, "Service", HID_UUID.to_string());
    insert!(options, "RequireAuthentication", false);
    insert!(options, "RequireAuthorization", false);

    let my_uuid = uuid::Uuid::new_v4().to_string();
    proxy.register_profile(dbus::Path::from(HID_PATH), &my_uuid, options)?;

    adapter.set_pairable(true)?;
    adapter.set_discoverable(true)?;

    std::thread::sleep(Duration::from_secs(1));
    unsafe {
        hci_device.write_class(0x002508);
    }

    println!("Connecting with the Switch... Please open the \"Change Grip/Order\" menu.");

    let (switch_ctl_l2cap, ctl_addr) = ctl_server_l2cap.accept()?;
    let (switch_itr_l2cap, itr_addr) = itr_server_l2cap.accept()?;

    assert_eq!(ctl_addr.l2_bdaddr.b, itr_addr.l2_bdaddr.b);

    let address = BtAddr(ctl_addr.l2_bdaddr.b).convert_host_byteorder();

    println!("Connected to switch at {}", address);

    println!("Forwarding all data from controller to switch. Exit the change grip menu even if it hasn't paired yet.");

    let switch_ctl = smol::Async::new(switch_ctl_l2cap).unwrap();
    let switch_itr = smol::Async::new(switch_itr_l2cap).unwrap();

    println!("Stopping flush");

    stop_tx.send(()).unwrap();

    controller_itr_l2cap = flush_thread.join().unwrap().unwrap();

    let controller_ctl = smol::Async::new(controller_ctl_l2cap).unwrap();
    let controller_itr = smol::Async::new(controller_itr_l2cap).unwrap();

    println!("Stopped flush");


    // println!("This little maneuver is about to cost us 15ms");

    // let adapter_devid = libc_check_error(unsafe {
    //     libbluetooth::hci_lib::hci_devid(adapter_addr_str.as_ptr() as *const i8)
    // }).unwrap();

    // let adapter_devid = 0;

    // println!("Adapter device id: {:?}", adapter_devid);

    // let adapter_dd = libc_check_error(unsafe {
    //     libbluetooth::hci_lib::hci_open_dev(adapter_devid as i32)
    // }).unwrap();

    // let controller_dd = libc_check_error(unsafe {
    //     libbluetooth::hci_lib::hci_open_dev(controller_devid as i32)
    // }).unwrap();

    // let n_15ms = 24; // 24 * 0.625 = 15

    // 1  =  0.625ms
    // 24 = 15.000ms
    // hci_utils::hci_sniff_mode(
    //     adapter_dd, 0x1, 100, 4,
    //     40, 40, 1000
    // ).unwrap();

    // std::thread::sleep(Duration::from_secs(10));
    // std::process::exit(0);



    // let ctl_relay = std::thread::spawn(move || {
    //     let (mut sw_ctl_r, mut sw_ctl_w) = switch_ctl.split();
    //     let (mut cn_ctl_r, mut cn_ctl_w) = controller_ctl.split();

    //     smol::run(async {
    //         let mut controller_incoming = [0u8; 512];
    //         let mut switch_incoming = [0u8; 512];

    //         let mut sw_r = sw_ctl_r.read(&mut switch_incoming);
    //         let mut cn_r = cn_ctl_r.read(&mut controller_incoming);

    //         let mut total_read_from_cn = 0;
    //         let mut total_read_from_sw = 0;

    //         loop {
    //             let both_r = futures::future::select(sw_r, cn_r);

    //             match both_r.await {
    //                 // Read successfully from switch
    //                 future::Either::Left((Ok(n), old_cn_r)) => {
    //                     total_read_from_sw += n;

    //                     if n == 0 {
    //                         println!("Read 0 bytes from switch ctl. Closing");
    //                         break;
    //                     }

    //                     cn_ctl_w.write_all(&switch_incoming[0..n]).await.unwrap();

    //                     cn_r = old_cn_r;
    //                     sw_r = sw_ctl_r.read(&mut switch_incoming);
    //                 },

    //                 // Read successfully from controller
    //                 future::Either::Right((Ok(n), old_sw_r)) => {
    //                     total_read_from_cn += n;

    //                     if n == 0 {
    //                         println!("Read 0 bytes from controll itr. Closing");
    //                         break;
    //                     }

    //                     // dbg!(n);

    //                     sw_ctl_w.write_all(&controller_incoming[0..n]).await.unwrap();

    //                     sw_r = old_sw_r;
    //                     cn_r = cn_ctl_r.read(&mut controller_incoming);
    //                 },

    //                 // Read failed from switch
    //                 future::Either::Left((Err(e), _old_cn_r)) => {
    //                     println!("Read from switch failed: {}", e);
    //                     break;
    //                 },

    //                 // Read failed from controller
    //                 future::Either::Right((Err(e), _old_sw_r)) => {
    //                     println!("Read from controller failed: {}", e);
    //                     break;
    //                 },
    //             };
    //         }

    //         println!("CTL finished.");
    //         println!("Total bytes from from controller: {}", total_read_from_cn);
    //         println!("Total bytes from from switch    : {}", total_read_from_sw);
    //     });
    // });


    // let itr_relay = std::thread::spawn(move || {
    let (mut sw_itr_r, mut sw_itr_w) = switch_itr.split();
    let (mut cn_itr_r, mut cn_itr_w) = controller_itr.split();

    smol::run(async {
        let mut controller_incoming = [0u8; 128];
        let mut switch_incoming = [0u8; 128];

        let mut last_cn_len = 0;
        let mut last_sw_len = 0;

        let mut sw_r = sw_itr_r.read(&mut switch_incoming);
        let mut cn_r = cn_itr_r.read(&mut controller_incoming);

        let mut total_read_from_cn = 0;
        let mut total_read_from_sw = 0;

        loop {
            let both_r = futures::future::select(sw_r, cn_r);

            match both_r.await {
                // Read successfully from switch
                future::Either::Left((Ok(n), old_cn_r)) => {
                    total_read_from_sw += n;
                    last_sw_len = n;

                    if n == 0 {
                        println!("Read 0 bytes from switch itr. Closing");
                        break;
                    }

                    cn_itr_w.write_all(&switch_incoming[0..n]).await.unwrap();

                    cn_r = old_cn_r;
                    sw_r = sw_itr_r.read(&mut switch_incoming);
                }

                // Read successfully from controller
                future::Either::Right((Ok(mut n), old_sw_r)) => {
                    total_read_from_cn += n;
                    last_cn_len = n;

                    if n == 0 {
                        println!("Read 0 bytes from controller itr. Closing");
                        break;
                    }

                    // dbg!(n);

                    let hid_packet = &mut controller_incoming[1..n];

                    if n == 50 && (hid_packet[0], hid_packet[14]) == (0x21, 0x02) {
                        println!("Got a device info packet.");

                        let mut old_addr = BtAddr([0; 6]);
                        old_addr.0.copy_from_slice(&hid_packet[19..25]);

                        println!("Old address: {}", old_addr);

                        hid_packet[19..25].copy_from_slice(&adapter_addr.0[..]);

                        println!("New address: {}", adapter_addr);
                    }

                    sw_itr_w.write_all(&controller_incoming[0..n]).await.unwrap();

                    sw_r = old_sw_r;
                    cn_r = cn_itr_r.read(&mut controller_incoming);
                }

                // Read failed from switch
                future::Either::Left((Err(e), _old_cn_r)) => {
                    println!("Read from switch failed: {}", e);
                    break;
                }

                // Read failed from controller
                future::Either::Right((Err(e), _old_sw_r)) => {
                    println!("Read from controller failed: {}", e);
                    break;
                }
            };
        }

        println!("ITR finished.");
        println!("Dumping last read from controller");
        println!("{}", hexdump(&controller_incoming[..last_cn_len]));

        println!("Dumping last read from switch");
        println!("{}", hexdump(&switch_incoming[..last_sw_len]));

        println!("Total bytes from from controller: {}", total_read_from_cn);
        println!("Total bytes from from switch    : {}", total_read_from_sw);
    });
    // });

    // ctl_relay.join().unwrap();
    // itr_relay.join().unwrap();

    // Everything is closed on drop

    Ok(())
}

use std::fmt::Write as FmtWrite;
use std::borrow::BorrowMut;
use crate::libc_helpe::libc_check_error;

fn hexdump(buf: &[u8]) -> String {
    let mut out = String::with_capacity(buf.len() * 4);

    for chunk in buf.chunks(16) {
        for byte in chunk {
            write!(out, "{:02x} ", byte);
        }

        for _ in 0..16 - chunk.len() {
            out.push_str("   ");
        }

        out.push(' ');

        for byte in chunk {
            let c = *byte as char;

            if c.is_alphanumeric() {
                out.push(c);
            } else {
                out.push('.');
            }
        }

        out.push('\n');
    }

    out
}
