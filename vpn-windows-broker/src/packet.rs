use anyhow::{ensure, Result};

pub const MAX_PACKET: usize = 65_535;

/// A message pipe frame contains exactly one IP packet, without a length prefix.
pub fn validate(packet: &[u8], mtu: usize) -> Result<()> {
    ensure!((576..=MAX_PACKET).contains(&mtu), "invalid_packet_mtu");
    ensure!(!packet.is_empty() && packet.len() <= mtu, "packet_size_invalid");
    match packet[0] >> 4 {
        4 => {
            ensure!(packet.len() >= 20, "ipv4_header_truncated");
            let header = (packet[0] as usize & 15) * 4;
            ensure!(header >= 20 && header <= packet.len(), "ipv4_header_invalid");
            let length = u16::from_be_bytes([packet[2], packet[3]]) as usize;
            ensure!(length == packet.len(), "ipv4_length_mismatch");
        }
        6 => {
            ensure!(packet.len() >= 40, "ipv6_header_truncated");
            let payload = u16::from_be_bytes([packet[4], packet[5]]) as usize;
            ensure!(payload + 40 == packet.len(), "ipv6_length_mismatch");
        }
        _ => anyhow::bail!("invalid_ip_version"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frames_are_one_complete_ip_packet_and_bounded_by_mtu() {
        let mut v4 = vec![0; 20];
        v4[0] = 0x45;
        v4[3] = 20;
        assert!(validate(&v4, 1500).is_ok());
        let mut two = v4.clone();
        two.extend(&v4);
        assert!(validate(&two, 1500).is_err());
        v4[0] = 0x44;
        assert!(validate(&v4, 1500).is_err());
        let mut v6 = vec![0; 40];
        v6[0] = 0x60;
        assert!(validate(&v6, 1500).is_ok());
        v6[5] = 1;
        assert!(validate(&v6, 1500).is_err());
        assert!(validate(&[], 1500).is_err());
        let mut large = vec![0; 1501];
        large[0] = 0x45;
        large[2..4].copy_from_slice(&1501_u16.to_be_bytes());
        assert!(validate(&large, 1500).is_err());
        assert!(validate(&[0x30], 1500).is_err());
    }
}
