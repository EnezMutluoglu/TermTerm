#ifdef NDEBUG
#undef NDEBUG
#endif
#include "packet.hpp"
#include <array>
#include <cassert>

int main() {
    std::array<std::uint8_t, 20> ipv4{};
    ipv4[0] = 0x45; ipv4[3] = 20;
    assert(valid_ip_packet(ipv4));
    assert(!valid_ip_packet(ipv4, 19));
    ipv4[0] = 0x44;
    assert(!valid_ip_packet(ipv4));
    ipv4[0] = 0x46;
    assert(!valid_ip_packet(ipv4));
    ipv4[0] = 0x45; ipv4[3] = 21;
    assert(!valid_ip_packet(ipv4));
    std::array<std::uint8_t, 40> ipv6{};
    ipv6[0] = 0x60;
    assert(valid_ip_packet(ipv6));
    ipv6[5] = 1;
    assert(!valid_ip_packet(ipv6));
    ipv6[0] = 0x70;
    assert(!valid_ip_packet(ipv6));
    assert(!valid_ip_packet({}));
}
