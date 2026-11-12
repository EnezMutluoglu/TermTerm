#pragma once
#include "packet.hpp"
#include <openvpn/tun/extern/config.hpp>
#include <openvpn/tun/tunmtu.hpp>
#include <asio/windows/stream_handle.hpp>
#include <array>
#include <deque>
#include <memory>
#include <vector>

// No adapter or network API is reachable from this factory; the broker owns both.
class PacketTun : public openvpn::TunClient {
    using Ptr = openvpn::RCPtr<PacketTun>;
    Client &client;
    openvpn::ExternalTun::Config config;
    openvpn::TunClientParent &parent;
    openvpn_io::windows::stream_handle pipe;
    openvpn::TunProp::State::Ptr state{new openvpn::TunProp::State()};
    std::array<std::uint8_t, 65535> incoming{};
    std::deque<std::shared_ptr<std::vector<std::uint8_t>>> outgoing;
    bool stopped = true;

    void fail(const std::string &reason) {
        stop();
        parent.tun_error(openvpn::Error::TUN_ERROR, reason);
    }
    void read() {
        pipe.async_read_some(openvpn_io::buffer(incoming), [self = Ptr(this)](const openvpn_io::error_code &error, std::size_t size) {
            if (self->stopped) return;
            if (error || !valid_ip_packet({self->incoming.data(), size}, self->vpn_mtu())) {
                self->fail("packet_pipe_read_failed"); return;
            }
            try {
                openvpn::BufferAllocated packet;
                self->config.frame->prepare(openvpn::Frame::READ_TUN, packet);
                packet.write(self->incoming.data(), size);
                self->parent.tun_recv(packet);
                if (!self->stopped) self->read();
            } catch (const std::exception &) { self->fail("packet_pipe_receive_failed"); }
        });
    }
    void write() {
        auto packet = outgoing.front();
        pipe.async_write_some(openvpn_io::buffer(*packet), [self = Ptr(this), packet](const openvpn_io::error_code &error, std::size_t size) {
            if (self->stopped) return;
            if (error || size != packet->size()) { self->fail("packet_pipe_write_failed"); return; }
            self->outgoing.pop_front();
            if (!self->outgoing.empty()) self->write();
        });
    }
public:
    PacketTun(openvpn_io::io_context &context, Client &owner, const openvpn::ExternalTun::Config &conf, openvpn::TunClientParent &p)
        : client(owner), config(conf), parent(p), pipe(context) {
        if (config.tun_prop.mtu == 0) config.tun_prop.mtu = openvpn::TUN_MTU_DEFAULT;
    }
    ~PacketTun() override { stop(); }
    void tun_start(const openvpn::OptionList &options, openvpn::TransportClient &transport, openvpn::CryptoDCSettings &) override {
        try {
            if (!stopped || !client.tun_builder_new()) throw std::runtime_error("packet_tunnel_reconnect_rejected");
            parent.tun_pre_tun_config();
            openvpn::TunProp::configure_builder(&client, state.get(), config.stats.get(), transport.server_endpoint_addr(), config.tun_prop, options, nullptr, false);
            // Core leaves state MTU unset when the server does not push tun-mtu.
            state->mtu = effective_packet_mtu(state->mtu, config.tun_prop.mtu);
            auto handle = client.establish_packet_pipe();
            openvpn_io::error_code error;
            pipe.assign(handle, error);
            if (error) { CloseHandle(handle); throw std::runtime_error("packet_pipe_assign_failed"); }
            stopped = false;
            read();
            parent.tun_connected();
        } catch (const std::exception &error) { fail(error.what()); }
    }
    void stop() override {
        stopped = true;
        openvpn_io::error_code ignored;
        pipe.cancel(ignored);
        pipe.close(ignored);
        outgoing.clear();
    }
    void set_disconnect() override {}
    bool tun_send(openvpn::BufferAllocated &packet) override {
        if (stopped) return false;
        if (!valid_ip_packet({packet.c_data(), packet.size()}, vpn_mtu()) || outgoing.size() >= 64) {
            fail("invalid_packet_or_queue_full"); return false;
        }
        const bool idle = outgoing.empty();
        outgoing.push_back(std::make_shared<std::vector<std::uint8_t>>(packet.c_data(), packet.c_data() + packet.size()));
        if (idle) write();
        return true;
    }
    std::string tun_name() const override { return "termterm-broker-packet-pipe"; }
    std::string vpn_ip4() const override { return state->vpn_ip4_addr.specified() ? state->vpn_ip4_addr.to_string() : ""; }
    std::string vpn_ip6() const override { return state->vpn_ip6_addr.specified() ? state->vpn_ip6_addr.to_string() : ""; }
    std::string vpn_gw4() const override { return state->vpn_ip4_gw.specified() ? state->vpn_ip4_gw.to_string() : ""; }
    std::string vpn_gw6() const override { return state->vpn_ip6_gw.specified() ? state->vpn_ip6_gw.to_string() : ""; }
    int vpn_mtu() const override { return state->mtu; }
};

class PacketFactory : public openvpn::TunClientFactory {
    Client &client;
    openvpn::ExternalTun::Config config;
public:
    PacketFactory(Client &owner, const openvpn::ExternalTun::Config &conf) : client(owner), config(conf) {}
    bool supports_epoch_data() override { return true; }
    openvpn::TunClient::Ptr new_tun_client_obj(openvpn_io::io_context &context, openvpn::TunClientParent &parent, openvpn::TransportClient *) override {
        return new PacketTun(context, client, config, parent);
    }
};

openvpn::TunClientFactory *Client::new_tun_factory(const openvpn::ExternalTun::Config &conf, const openvpn::OptionList &) {
    return new PacketFactory(*this, conf);
}
