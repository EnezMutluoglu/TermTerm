// OpenVPN Core is included under its MPL-2.0 option; upstream files are unchanged.
#define OPENVPN_CORE_API_VISIBILITY_HIDDEN
#include <client/ovpncli.cpp>
#include "channel.hpp"
#include <thread>
#include <atomic>
#include <poll.h>
#include <unistd.h>

class Client : public openvpn::ClientAPI::OpenVPNClient {
public:
    void event(const openvpn::ClientAPI::Event &e) override {
        Json::Value detail;
        detail["name"] = e.name;
        detail["error"] = e.error;
        // Raw engine messages can contain authentication cookies; never relay them.
        if (e.name == "CONNECTED") {
            auto info = connection_info();
            detail["address"] = info.vpnIp4;
            detail["address6"] = info.vpnIp6;
            ::event("connected", detail);
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
        Client client;
        const auto parsed = client.eval_config(config);
        if (parsed.error) throw std::runtime_error("invalid_openvpn_configuration");
        if (parsed.externalPki) throw std::runtime_error("external_pki_unsupported");
        if (!parsed.autologin) {
            Json::Value detail, fields(Json::arrayValue);
            for (const auto *name : {"username", "password"}) {
                Json::Value f;
                f["name"] = name; f["label"] = name; f["secret"] = std::string(name) == "password";
                fields.append(f);
            }
            detail["id"] = 1; detail["fields"] = fields;
            ::event("challenge", detail);
            auto answer = read_message();
            if (answer["action"] != "answer" || answer["id"] != 1) throw std::runtime_error("cancelled");
            openvpn::ClientAPI::ProvideCreds creds;
            creds.username = answer["values"]["username"].asString();
            creds.password = answer["values"]["password"].asString();
            if (client.provide_creds(creds).error) throw std::runtime_error("invalid_credentials");
            std::fill(creds.password.begin(), creds.password.end(), '\0');
            answer = Json::nullValue;
        }
        std::atomic<bool> finished{false};
        std::thread control([&] {
            while (!finished) {
                pollfd fd{STDIN_FILENO, POLLIN, 0};
                if (poll(&fd, 1, 100) > 0) { client.stop(); break; }
            }
        });
        ::event("connecting");
        const auto status = client.connect();
        finished = true;
        control.join();
        ::event(status.error ? "failed" : "disconnected", status.status);
        return status.error ? 1 : 0;
    } catch (const std::exception &error) {
        ::event("failed", error.what());
        return 1;
    }
}
