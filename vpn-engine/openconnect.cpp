// The library retains its LGPL-2.1 license and is dynamically linked.
#include "channel.hpp"
#include "tun-builder.hpp"
#include <openconnect.h>
#include <unistd.h>
#include <poll.h>
#include <thread>
#include <atomic>
#include <memory>
#include <set>

static unsigned challenge = 0;
struct AuthState { std::string realm; bool group_selected = false; };
static int auth(void *context, oc_auth_form *form) {
    auto &state = *static_cast<AuthState *>(context);
    const bool choose_group = form->authgroup_opt && !state.group_selected;
    if (choose_group && !state.realm.empty()) {
        auto *group = form->authgroup_opt;
        for (int i = 0; i < group->nr_choices; ++i) {
            if (state.realm == safe_text(group->choices[i]->name) || state.realm == safe_text(group->choices[i]->label)) {
                if (openconnect_set_option_value(&group->form, group->choices[i]->name)) return OC_FORM_RESULT_ERR;
                state.group_selected = true;
                return OC_FORM_RESULT_NEWGROUP;
            }
        }
        // A stale saved realm must be selected again, never silently substituted.
    }
    Json::Value fields(Json::arrayValue), detail;
    for (auto *opt = form->opts; opt; opt = opt->next) {
        if (opt->flags & OC_FORM_OPT_IGNORE || opt->type == OC_FORM_OPT_HIDDEN) continue;
        if (choose_group && opt != &form->authgroup_opt->form) continue;
        if (!choose_group && form->authgroup_opt && opt == &form->authgroup_opt->form) continue;
        if (opt->type == OC_FORM_OPT_SSO_TOKEN || opt->type == OC_FORM_OPT_SSO_USER) {
            event("failed", "browser_authentication_unsupported");
            return OC_FORM_RESULT_CANCELLED;
        }
        Json::Value field;
        field["name"] = safe_text(opt->name);
        field["label"] = safe_text(opt->label);
        field["secret"] = opt->type == OC_FORM_OPT_PASSWORD || opt->type == OC_FORM_OPT_TOKEN;
        field["purpose"] = opt->type == OC_FORM_OPT_TOKEN ? "otp" :
            opt->type == OC_FORM_OPT_PASSWORD ? "secret" : opt->type == OC_FORM_OPT_SELECT ? "choice" : "text";
        field["choices"] = Json::arrayValue;
        if (opt->type == OC_FORM_OPT_SELECT) {
            auto *select = reinterpret_cast<oc_form_opt_select *>(opt);
            for (int i = 0; i < select->nr_choices; ++i) {
                Json::Value item;
                item["value"] = safe_text(select->choices[i]->name);
                item["label"] = safe_text(select->choices[i]->label);
                field["choices"].append(item);
            }
        }
        fields.append(field);
    }
    detail["id"] = ++challenge;
    detail["fields"] = fields;
    detail["message"] = safe_text(form->message);
    detail["error"] = safe_text(form->error);
    event("challenge", detail);
    try {
        auto answer = read_message();
        if (answer["action"] != "answer" || answer["id"].asUInt() != challenge)
            return OC_FORM_RESULT_CANCELLED;
        for (auto *opt = form->opts; opt; opt = opt->next) {
            if (opt->flags & OC_FORM_OPT_IGNORE || opt->type == OC_FORM_OPT_HIDDEN) continue;
            if (choose_group && opt != &form->authgroup_opt->form) continue;
            if (!choose_group && form->authgroup_opt && opt == &form->authgroup_opt->form) continue;
            auto &value = answer["values"][safe_text(opt->name)];
            if (!value.isString() || value.asString().size() > 4096 || value.asString().find('\0') != std::string::npos)
                return OC_FORM_RESULT_ERR;
            if (opt->type == OC_FORM_OPT_SELECT) {
                auto *select = reinterpret_cast<oc_form_opt_select *>(opt);
                bool found = false;
                for (int i = 0; i < select->nr_choices; ++i)
                    if (value == safe_text(select->choices[i]->name)) found = true;
                if (!found) return OC_FORM_RESULT_ERR;
            }
            if (openconnect_set_option_value(opt, value.asCString())) return OC_FORM_RESULT_ERR;
            value = Json::nullValue;
        }
        if (choose_group) { state.group_selected = true; return OC_FORM_RESULT_NEWGROUP; }
        return OC_FORM_RESULT_OK;
    } catch (...) { return OC_FORM_RESULT_CANCELLED; }
}
static int reject_cert(void *, const char *reason) {
    Json::Value detail;
    detail["error"] = "certificate_verification_failed";
    detail["reason"] = safe_text(reason);
    event("failed", detail);
    return -1;
}
static int reject_browser(openconnect_info *, const char *, void *) {
    event("failed", "browser_authentication_unsupported");
    return -1;
}
static void quiet(void *, int, const char *, ...) {}

int main(int argc, char **argv) {
    try {
        if (argc == 2 && std::string(argv[1]) == "--capabilities") {
            oc_vpn_proto *protocols = nullptr;
            const int count = openconnect_get_supported_protocols(&protocols);
            if (count < 0) throw std::runtime_error("protocol_discovery_failed");
            Json::Value detail;
            detail["protocols"] = Json::arrayValue;
            for (int i = 0; i < count; ++i) detail["protocols"].append(safe_text(protocols[i].name));
            openconnect_free_supported_protocols(protocols);
            detail["version"] = openconnect_get_version();
            detail["isolatedLabOnly"] = true;
            event("capabilities", detail);
            return 0;
        }
        require_isolated_lab();
        auto request = read_message();
        const auto protocol = request["protocol"].asString();
        const std::set<std::string> protocols{"anyconnect", "fortinet", "gp", "pulse", "nc", "f5"};
        if (!protocols.contains(protocol)) throw std::runtime_error("unsupported_protocol");
        const auto server = request["server"].asString();
        if (!server.starts_with("https://") || server.find('@') != std::string::npos)
            throw std::runtime_error("https_server_required");
        openconnect_init_ssl();
        AuthState auth_state{request.get("realm", "").asString()};
        if (auth_state.realm.size() > 4096 || auth_state.realm.find('\0') != std::string::npos)
            throw std::runtime_error("invalid_realm");
        std::unique_ptr<openconnect_info, decltype(&openconnect_vpninfo_free)> vpn(
            openconnect_vpninfo_new("TermTerm-VPN/1", reject_cert, nullptr, auth, quiet, &auth_state),
            openconnect_vpninfo_free);
        if (!vpn) throw std::runtime_error("engine_initialization_failed");
        openconnect_set_external_browser_callback(vpn.get(), reject_browser);
        openconnect_set_webview_callback(vpn.get(), reject_browser);
        if (openconnect_set_protocol(vpn.get(), protocol.c_str()) || openconnect_parse_url(vpn.get(), server.c_str()))
            throw std::runtime_error("invalid_server");
        if (request["caFile"].isString() && openconnect_set_cafile(vpn.get(), request["caFile"].asCString()))
            throw std::runtime_error("invalid_ca_file");
        event("connecting");
        if (openconnect_obtain_cookie(vpn.get())) throw std::runtime_error("authentication_failed_or_cancelled");
        if (openconnect_make_cstp_connection(vpn.get())) throw std::runtime_error("tunnel_negotiation_failed");
        const oc_ip_info *ip = nullptr;
        openconnect_get_ip_info(vpn.get(), &ip, nullptr, nullptr);
        if (!ip) throw std::runtime_error("missing_network_configuration");
        Json::Value network;
        network["address"] = safe_text(ip->addr);
        network["netmask"] = safe_text(ip->netmask);
        network["address6"] = safe_text(ip->netmask6);
        network["routes"] = Json::arrayValue;
        for (auto *r = ip->split_includes; r; r = r->next) network["routes"].append(safe_text(r->route));
        // An empty include list means full tunnel, not 'no routes'.
        if (!ip->split_includes) {
            if (ip->addr) network["routes"].append("0.0.0.0/0");
            if (ip->addr6 || ip->netmask6) network["routes"].append("::/0");
        }
        network["excludedRoutes"] = Json::arrayValue;
        for (auto *r = ip->split_excludes; r; r = r->next)
            network["excludedRoutes"].append(safe_text(r->route));
        network["dnsDomains"] = Json::arrayValue;
        for (auto *r = ip->split_dns; r; r = r->next)
            network["dnsDomains"].append(safe_text(r->route));
        network["searchDomain"] = safe_text(ip->domain);
        network["proxyPac"] = safe_text(ip->proxy_pac);
        network["remote"] = safe_text(ip->gateway_addr);
        network["mtu"] = ip->mtu;
        network["dns"] = Json::arrayValue;
        for (const auto *dns : ip->dns) if (dns) network["dns"].append(safe_text(dns));
        // Parent must check route/DNS conflicts before the system is changed.
        event("network-plan", network);
        auto approval = read_message();
        if (approval["action"] != "accept-network") throw std::runtime_error("network_plan_not_accepted");
        // This build is lab-only until a root-owned, authenticated broker is installed.
        if (geteuid() != 0) throw std::runtime_error("privileged_broker_required");
        const auto interface = request["interface"].asString();
        validate_interface(interface);
        // Network ownership belongs to the broker, never an arbitrary VPN script.
        int tun = create_tun(interface);
        if (openconnect_setup_tun_fd(vpn.get(), tun)) {
            close(tun);
            throw std::runtime_error("tun_setup_failed");
        }
        network["interface"] = interface;
        event("tunnel-ready", network);
        if (read_message()["action"] != "network-configured")
            throw std::runtime_error("network_configuration_failed");
        const int command = openconnect_setup_cmd_pipe(vpn.get());
        if (command < 0) throw std::runtime_error("control_pipe_failed");
        std::atomic<bool> finished{false};
        std::thread control([&] {
            while (!finished) {
                pollfd fd{STDIN_FILENO, POLLIN, 0};
                if (poll(&fd, 1, 100) > 0) {
                    char stop = OC_CMD_CANCEL;
                    (void)!write(command, &stop, 1);
                    break;
                }
            }
        });
        event("connected", network);
        int result = openconnect_mainloop(vpn.get(), 0, 1);
        finished = true;
        control.join();
        if (result != -EINTR && result != 0) event("failed", "vpn_transport_lost");
        event("disconnected");
        return result == -EINTR || result == 0 ? 0 : 1;
    } catch (const std::exception &error) {
        event("failed", error.what());
        return 1;
    }
}
