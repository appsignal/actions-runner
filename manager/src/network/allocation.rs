use config::NETWORK_MASK_SHORT;
use std::net::Ipv4Addr;
use std::process::Command;
use util::{exec, network::ip_to_mac, CommandExecutionError};

#[derive(Debug)]
pub struct NetworkAllocation {
    pub interface: String,
    pub host_ip: Ipv4Addr,
    pub guest_mac: String,
    pub client_ip: Ipv4Addr,
    pub tap_name: String,
}

impl NetworkAllocation {
    pub fn new(interface: &str, idx: u8) -> Self {
        let host_ip = Ipv4Addr::new(172, 16, idx, 1);
        let client_ip = Ipv4Addr::new(172, 16, idx, 2);
        Self {
            interface: interface.to_string(),
            guest_mac: ip_to_mac(&client_ip),
            tap_name: format!("tap{}", idx),
            host_ip,
            client_ip,
        }
    }

    pub fn setup(&self) -> Result<(), CommandExecutionError> {
        // Remove existing tap device
        let _ = exec(Command::new("ip").args(["link", "del", &self.tap_name]));

        // Create tap device
        let _ =
            exec(Command::new("ip").args(["tuntap", "add", "dev", &self.tap_name, "mode", "tap"]));

        // Add address to tap device
        let _ = exec(Command::new("ip").args([
            "addr",
            "add",
            &format!("{}/{}", self.host_ip, NETWORK_MASK_SHORT),
            "dev",
            &self.tap_name,
        ]));

        // Bring up tap device
        let _ = exec(Command::new("ip").args(["link", "set", "dev", &self.tap_name, "up"]));

        // Set up internet access
        let _ = exec(Command::new("iptables").args([
            "-I",
            "FORWARD",
            "1",
            "-i",
            &self.tap_name,
            "-o",
            &self.interface,
            "-j",
            "ACCEPT",
        ]));

        Ok(())
    }
}
