#ifdef NDEBUG
#undef NDEBUG
#endif
#include "packet.hpp"
#include <array>
#include <cassert>

int main() {
    assert(effective_packet_mtu(0, 1500) == 1500);
    assert(effective_packet_mtu(1400, 1500) == 1400);
    assert(effective_packet_mtu(576, 1500) == 576);
    assert(effective_packet_mtu(9000, 1500) == 9000);
    for (const auto mtu : {-1, 0, 575, 9001}) {
        bool rejected = false;
        try { effective_packet_mtu(mtu, mtu); }
        catch (const std::runtime_error &) { rejected = true; }
        assert(rejected);
    }
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
