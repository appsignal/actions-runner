use super::NetworkAllocation;
use config::NETWORK_MAX_ALLOCATIONS;
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AllocatorError {
    #[error("No free IPs")]
    NoFreeIps,
}

pub struct NetworkAllocator {
    pub interface: String,
    pub allocations: BTreeMap<u8, String>,
}

impl NetworkAllocator {
    pub fn new(interface: &str) -> Self {
        Self {
            interface: interface.to_string(),
            allocations: BTreeMap::new(),
        }
    }

    pub fn allocate(&self) -> Result<NetworkAllocation, AllocatorError> {
        for idx in 0..NETWORK_MAX_ALLOCATIONS {
            if !self.allocations.contains_key(&idx) {
                return Ok(NetworkAllocation::new(&self.interface, idx));
            }
        }
        Err(AllocatorError::NoFreeIps)
    }

    pub fn deallocate(&mut self, idx: &u8) -> Result<(), AllocatorError> {
        self.allocations.remove(idx);
        Ok(())
    }
}
