use std::process::Command;
use util::{exec, CommandExecutionError};

pub struct Forwarding {
    pub interface: String,
}

impl Forwarding {
    pub fn new(interface: &str) -> Self {
        Self {
            interface: interface.to_string(),
        }
    }

    pub fn setup(&self) -> Result<(), CommandExecutionError> {
        // Enable IP forwarding
        // sh -c "echo 1 > /proc/sys/net/ipv4/ip_forward"
        let _ = exec(Command::new("sh").args(["-c", "echo 1 > /proc/sys/net/ipv4/ip_forward"]));

        // Set up nat
        // iptables -t nat -A POSTROUTING -o $1 -j MASQUERADE
        let _ = exec(Command::new("iptables").args([
            "-t",
            "nat",
            "-A",
            "POSTROUTING",
            "-o",
            &self.interface,
            "-j",
            "MASQUERADE",
        ]));

        // Set up forwarding
        // iptables -I FORWARD 1 -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
        let _ = exec(Command::new("iptables").args([
            "-I",
            "FORWARD",
            "1",
            "-m",
            "conntrack",
            "--ctstate",
            "RELATED,ESTABLISHED",
            "-j",
            "ACCEPT",
        ]));
        Ok(())
    }
}
