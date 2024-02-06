use config::{NETWORK_MAGIC_MAC_START, NETWORK_MASK_SHORT};
use serde::Deserialize;
use serde_json;
use std::{fs::write, net::Ipv4Addr, process::Command};
use thiserror::Error;
use util::{exec, network::mac_to_ip};

const RESOLV_CONF: &str = "nameserver 1.1.1.1\noptions use-vc\n";
const RESOLV_CONF_PATH: &str = "/etc/resolv.conf";

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("IO error: {:?}", self)]
    Io(#[from] std::io::Error),
    #[error("UTF8 error: {:?}", self)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("Command error: {:?}", self)]
    Command(#[from] util::CommandExecutionError),
    #[error("JSON error: {:?}", self)]
    Json(#[from] serde_json::Error),
    #[error("No interface found with mac address starting with 06:00")]
    NoInterfaceFound,
    #[error("No valid IP adddress in mac address: {}", .0)]
    MacToIpError(#[from] util::network::MacToIpError),
}

#[derive(Deserialize, Debug)]
pub struct NetworkAddress {
    pub ifname: String,
    #[serde(rename = "address")]
    pub mac: String,
}

#[derive(Deserialize, Debug)]
pub struct NetworkInterface {
    pub ifname: String,
    pub mac: String,
    pub own_address: Ipv4Addr,
    pub host_address: Ipv4Addr,
}

pub fn get_interfaces() -> Result<Vec<NetworkAddress>, NetworkError> {
    let command_output = exec(Command::new("ip").arg("-j").arg("address"))?;
    let output_string = String::from_utf8_lossy(&command_output.stdout).to_string();
    let interfaces: Vec<NetworkAddress> = serde_json::from_str(&output_string)?;

    Ok(interfaces)
}

pub fn get_magic_address() -> Result<NetworkAddress, NetworkError> {
    for interface in get_interfaces()? {
        if interface.mac.starts_with(NETWORK_MAGIC_MAC_START) {
            return Ok(interface);
        }
    }
    Err(NetworkError::NoInterfaceFound.into())
}

pub fn setup_network() -> Result<Option<NetworkInterface>, NetworkError> {
    let magic_address = match get_magic_address() {
        Ok(i) => i,
        Err(_) => return Ok(None),
    };
    let own_ip = mac_to_ip(&magic_address.mac)?;
    let host_ip = Ipv4Addr::new(
        own_ip.octets()[0],
        own_ip.octets()[1],
        own_ip.octets()[2],
        1,
    );

    exec(Command::new("ip").args([
        "addr",
        "add",
        &format!("{}/{}", own_ip.to_string(), NETWORK_MASK_SHORT),
        "dev",
        &magic_address.ifname,
    ]))?;

    exec(Command::new("ip").args(["link", "set", &magic_address.ifname, "up"]))?;

    exec(Command::new("ip").args([
        "route",
        "add",
        "default",
        "via",
        host_ip.to_string().as_str(),
    ]))?;

    Ok(Some(NetworkInterface {
        ifname: magic_address.ifname.to_string(),
        mac: magic_address.mac.to_string(),
        own_address: own_ip,
        host_address: host_ip,
    }))
}

pub fn setup_dns() -> Result<(), NetworkError> {
    write(RESOLV_CONF_PATH, RESOLV_CONF)?;
    Ok(())
}
