use config::NETWORK_MAGIC_MAC_START;
use std::net::Ipv4Addr;
use std::num::ParseIntError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MacToIpError {
    #[error("Could not parse octed from string: {}", self)]
    ParseIntError(#[from] ParseIntError),
    #[error("No valid IP adddress in mac address: {}", .0)]
    NoIpInMac(String),
}

// Convert an IP address to a MAC address
pub fn ip_to_mac(ip: &Ipv4Addr) -> String {
    format!(
        "{}:{:02x}:{:02x}:{:02x}:{:02x}",
        NETWORK_MAGIC_MAC_START,
        ip.octets()[0],
        ip.octets()[1],
        ip.octets()[2],
        ip.octets()[3]
    )
}

// Convert an IP address to a MAC address
pub fn mac_to_ip(mac: &str) -> Result<Ipv4Addr, MacToIpError> {
    let octets = mac
        .replace("06:00:", "")
        .split(':')
        .map(|octet| u8::from_str_radix(octet, 16))
        .collect::<Result<Vec<u8>, ParseIntError>>()
        .map_err(|_| MacToIpError::NoIpInMac(mac.to_string()))?;

    let address = Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3]);
    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mac_to_ip() {
        let mac = "06:00:ac:10:c9:01";
        let ip = super::mac_to_ip(mac).unwrap();
        assert_eq!(ip, Ipv4Addr::new(172, 16, 201, 1));
    }

    #[test]
    fn test_ip_to_mac() {
        assert_eq!(
            ip_to_mac(&Ipv4Addr::new(172, 16, 0, 1)),
            "06:00:ac:10:00:01"
        );

        assert_eq!(
            ip_to_mac(&Ipv4Addr::new(172, 16, 10, 2)),
            "06:00:ac:10:0a:02"
        );
    }
}
