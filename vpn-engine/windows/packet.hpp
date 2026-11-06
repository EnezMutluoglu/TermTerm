#pragma once
#include <cstddef>
#include <cstdint>
#include <span>

inline bool valid_ip_packet(std::span<const std::uint8_t> packet, std::size_t mtu = 65535) {
    if (packet.size() < 20 || packet.size() > 65535 || packet.size() > mtu) return false;
    if (packet[0] >> 4 == 4) {
        const auto header = (packet[0] & 15) * 4;
        return header >= 20 && header <= packet.size() &&
               ((static_cast<unsigned>(packet[2]) << 8) | packet[3]) == packet.size();
    }
    return packet[0] >> 4 == 6 && packet.size() >= 40 &&
           ((static_cast<unsigned>(packet[4]) << 8) | packet[5]) + 40 == packet.size();
}
