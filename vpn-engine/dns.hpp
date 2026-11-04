#pragma once
#include <openvpn/client/dns_options.hpp>

// Only the pinned core's basic legacy policy has an adapter-DNS equivalent.
// Preserve every other policy for the broker's fail-closed unsupported guard.
inline bool project_legacy_dns(const Json::Value &raw, Json::Value &network) {
    try {
        const openvpn::DnsOptions dns(raw);
        const Json::StreamWriterBuilder writer;
        if (Json::writeString(writer,dns.to_json()) != Json::writeString(writer,raw) || !dns.from_dhcp_options || dns.servers.size() != 1
            || dns.servers.begin()->first != 0 || dns.search_domains.size() > 1) return false;
        const auto &server = dns.servers.begin()->second;
        if (server.addresses.empty() || server.addresses.size() > 8 || !server.domains.empty()
            || server.dnssec != openvpn::DnsServer::Security::Unset
            || server.transport != openvpn::DnsServer::Transport::Unset || !server.sni.empty()) return false;
        Json::Value addresses(Json::arrayValue);
        for (const auto &address : server.addresses) {
            if ((address.port != 0 && address.port != 53) || openvpn::IP::Addr::from_string(address.address).version() != openvpn::IP::Addr::V4) return false;
            addresses.append(address.address);
        }
        network["dns"] = std::move(addresses);
        network["searchDomain"] = dns.search_domains.empty() ? "" : dns.search_domains.front().domain;
        network["dnsOptions"] = Json::nullValue;
        return true;
    } catch (...) { return false; }
}
