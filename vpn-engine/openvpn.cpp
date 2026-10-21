// OpenVPN Core is included under its MPL-2.0 option; upstream files are unchanged.
#define OPENVPN_CORE_API_VISIBILITY_HIDDEN
#define USE_TUN_BUILDER
#define HAVE_JSON
#include <client/ovpncli.hpp>
#include "channel.hpp"
#include "tun-builder.hpp"
#include <thread>
#include <atomic>
#include <poll.h>
#include <unistd.h>

static unsigned challenge_id = 0;
static Json::Value field(const char *name, const std::string &label, bool secret, const char *purpose) {
    Json::Value f;
    f["name"] = name; f["label"] = label; f["secret"] = secret; f["purpose"] = purpose;
    return f;
}
static Json::Value ask(const Json::Value &fields) {
    Json::Value detail;
    detail["id"] = ++challenge_id; detail["fields"] = fields;
    ::event("challenge", detail);
    auto answer = read_message();
    if (answer["action"] != "answer" || !answer["id"].isUInt() || answer["id"].asUInt() != challenge_id)
        throw std::runtime_error("cancelled");
    auto values = answer["values"];
    for (const auto &f : fields) {
        const auto &value = values[f["name"].asString()];
        if (!value.isString() || value.asString().size() > 4096 || value.asString().find('\0') != std::string::npos)
            throw std::runtime_error("invalid_authentication_answer");
    }
    return values;
}

class Client : public openvpn::ClientAPI::OpenVPNClient {
public:
    std::string interface;
    Json::Value network;
    std::atomic<bool> finished{false};
    std::thread control;
    std::string dynamic_cookie;
    bool connected_once = false;
    ~Client() { finish_control(); }
    void finish_control() {
        finished = true;
        if (control.joinable()) control.join();
    }
    bool tun_builder_new() override {
        // Reconnection requires a fresh broker reservation and connection generation.
        if (connected_once) { stop(); return false; }
        network = Json::objectValue;
        for (auto key : {"addresses", "routes", "excludedRoutes", "dns", "dnsDomains"})
            network[key] = Json::arrayValue;
        network["interface"] = interface;
        return true;
    }
    bool tun_builder_set_remote_address(const std::string &address, bool) override {
        network["remote"] = address; return true;
    }
    bool tun_builder_set_layer(int layer) override { return layer == 3; }
    bool tun_builder_add_address(const std::string &address, int prefix,
                                 const std::string &gateway, bool, bool net30) override {
        Json::Value value;
        value["cidr"] = address + "/" + std::to_string(prefix);
        value["gateway"] = gateway; value["net30"] = net30;
        network["addresses"].append(value); return true;
    }
    bool tun_builder_add_route(const std::string &address, int prefix, int, bool) override {
        network["routes"].append(address + "/" + std::to_string(prefix)); return true;
    }
    bool tun_builder_exclude_route(const std::string &address, int prefix, int, bool) override {
        network["excludedRoutes"].append(address + "/" + std::to_string(prefix)); return true;
    }
    bool tun_builder_reroute_gw(bool ipv4, bool ipv6, unsigned) override {
        if (ipv4) network["routes"].append("0.0.0.0/0");
        if (ipv6) network["routes"].append("::/0");
        return true;
    }
    bool tun_builder_set_dns_options(const openvpn::DnsOptions &dns) override {
        // Preserve the full policy for the broker; never silently ignore split DNS.
        network["dnsOptions"] = dns.to_json();
        for (const auto &[priority, server] : dns.servers)
            for (const auto &address : server.addresses) {
                if (address.port != 0 && address.port != 53) return false;
                network["dns"].append(address.address);
            }
        return true;
    }
    bool tun_builder_set_mtu(int mtu) override { network["mtu"] = mtu; return true; }
    bool tun_builder_set_session_name(const std::string &) override { return true; }
    // IP-family blocking needs a broker firewall rule, not a successful no-op.
    bool tun_builder_set_allow_family(int, bool allow) override { return allow; }
    bool tun_builder_persist() override { return false; }
    int tun_builder_establish() override {
        int fd = -1;
        try {
            ::event("network-plan", network);
            if (read_message()["action"] != "accept-network")
                throw std::runtime_error("network_plan_not_accepted");
            fd = create_tun(interface);
            ::event("tunnel-ready", network);
            if (read_message()["action"] != "network-configured")
                throw std::runtime_error("network_configuration_failed");
            return fd;
        } catch (const std::exception &error) {
            if (fd >= 0) close(fd);
            ::event("failed", error.what()); return -1;
        }
    }
    void event(const openvpn::ClientAPI::Event &e) override {
        if (connected_once && (e.error || e.name == "RECONNECTING")) {
            ::event("failed", "vpn_transport_lost");
            stop();
            return;
        }
        if (e.name == "DYNAMIC_CHALLENGE") {
            // Session cookie stays inside the helper, never in a UI event/log.
            dynamic_cookie = e.info;
            return;
        }
        Json::Value detail;
        detail["name"] = e.name;
        detail["error"] = e.error;
        // Raw engine messages can contain authentication cookies; never relay them.
        if (e.name == "CONNECTED") {
            connected_once = true;
            auto info = connection_info();
            detail["address"] = info.vpnIp4;
            detail["address6"] = info.vpnIp6;
            ::event("connected", detail);
            if (!control.joinable()) control = std::thread([this] {
                while (!finished) {
                    pollfd fd{STDIN_FILENO, POLLIN, 0};
                    if (poll(&fd, 1, 100) > 0) { stop(); break; }
                }
            });
        } else ::event(e.error ? "failed" : "progress", detail);
    }
    void acc_event(const openvpn::ClientAPI::AppCustomControlMessageEvent &) override {}
    void log(const openvpn::ClientAPI::LogInfo &) override {}
    bool pause_on_connection_timeout() override { return false; }
    void external_pki_cert_request(openvpn::ClientAPI::ExternalPKICertRequest &r) override { r.error = true; }
    void external_pki_sign_request(openvpn::ClientAPI::ExternalPKISignRequest &r) override { r.error = true; }
};

int main() {
    try {
        require_isolated_lab();
        const auto request = read_message();
        openvpn::ClientAPI::Config config;
        config.content = request["config"].asString();
        config.connTimeout = 20;
        config.retryOnAuthFailed = false;
        openvpn::ClientAPI::ProvideCreds creds;
        for (unsigned attempt = 0; attempt < 4; ++attempt) {
            Client client;
            client.interface = request["interface"].asString();
            validate_interface(client.interface);
            auto parsed = client.eval_config(config);
            if (parsed.error) throw std::runtime_error("invalid_openvpn_configuration");
            if (parsed.externalPki) throw std::runtime_error("external_pki_unsupported");
            if (parsed.privateKeyPasswordRequired && config.privateKeyPassword.empty()) {
                Json::Value fields(Json::arrayValue);
                fields.append(field("privateKeyPassword", "Özel anahtar parolası", true, "keyPassword"));
                auto answer = ask(fields);
                config.privateKeyPassword = answer["privateKeyPassword"].asString();
                answer = Json::nullValue;
                parsed = client.eval_config(config);
                if (parsed.error) throw std::runtime_error("private_key_unlock_failed");
            }
            if (!parsed.autologin) {
                if (attempt == 0) {
                    Json::Value fields(Json::arrayValue);
                    fields.append(field("username", "Kullanıcı adı", false, "username"));
                    fields.append(field("password", "Parola", true, "password"));
                    if (!parsed.staticChallenge.empty())
                        fields.append(field("response", parsed.staticChallenge, !parsed.staticChallengeEcho, "otp"));
                    auto answer = ask(fields);
                    creds.username = answer["username"].asString();
                    creds.password = answer["password"].asString();
                    creds.response = answer.get("response", "").asString();
                    answer = Json::nullValue;
                }
                if (client.provide_creds(creds).error) throw std::runtime_error("invalid_credentials");
                std::fill(creds.password.begin(), creds.password.end(), '\0');
                creds.password.clear();
                std::fill(creds.response.begin(), creds.response.end(), '\0');
                creds.response.clear();
            }
            ::event("connecting");
            const auto status = client.connect();
            client.finish_control();
            if (!client.dynamic_cookie.empty()) {
                openvpn::ClientAPI::DynamicChallenge dc;
                if (!openvpn::ClientAPI::OpenVPNClientHelper::parse_dynamic_challenge(client.dynamic_cookie, dc))
                    throw std::runtime_error("invalid_dynamic_challenge");
                Json::Value fields(Json::arrayValue);
                if (dc.responseRequired) fields.append(field("response", dc.challenge, !dc.echo, "otp"));
                auto answer = ask(fields);
                creds.response = answer.get("response", "").asString();
                creds.dynamicChallengeCookie = client.dynamic_cookie;
                continue;
            }
            std::fill(config.privateKeyPassword.begin(), config.privateKeyPassword.end(), '\0');
            ::event(status.error ? "failed" : "disconnected", status.status);
            return status.error ? 1 : 0;
        }
        throw std::runtime_error("authentication_round_limit");
    } catch (const std::exception &error) {
        ::event("failed", error.what());
        return 1;
    }
}
