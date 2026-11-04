// The library retains its LGPL-2.1 license and is dynamically linked.
#include "channel.hpp"
#ifdef _WIN32
#include "windows/packet-handle.hpp"
#include "windows/openconnect-api.hpp"
#else
#include "tun-builder.hpp"
#include <openconnect.h>
#include <unistd.h>
#include <poll.h>
#endif
#include <thread>
#include <atomic>
#include <memory>
#include <set>
#include <cctype>
#include "sso.hpp"
#include "auth-diagnostics.hpp"

static unsigned challenge = 0;
struct AuthState { std::string realm; bool group_selected = false; std::string protocol; bool gateway = false; AuthDiagnostics diagnostic; };
static int auth(void *context, oc_auth_form *form) {
    auto &state = *static_cast<AuthState *>(context);
    auto &diagnostic = state.diagnostic;
    diagnostic.stage = AuthStage::form;
    diagnostic.callback = AuthCallback::waiting;
    ++diagnostic.forms;
    for (auto *opt = form->opts; opt; opt = opt->next) {
        if (opt->type == OC_FORM_OPT_SSO_TOKEN) {
            // The library invokes the webview callback after this form callback.
            if (state.protocol == "gp") return diagnostic.returned(OC_FORM_RESULT_OK, AuthCallback::accepted);
            return diagnostic.returned(OC_FORM_RESULT_CANCELLED, AuthCallback::unsupported_browser);
        }
    }
    const bool choose_group = form->authgroup_opt && !state.group_selected;
    // GP gateway choices must remain observable so the broker can bind the next SSO origin.
    if (choose_group && !state.realm.empty() && state.protocol != "gp") {
        auto *group = form->authgroup_opt;
        for (int i = 0; i < group->nr_choices; ++i) {
            if (state.realm == safe_text(group->choices[i]->name) || state.realm == safe_text(group->choices[i]->label)) {
                if (openconnect_set_option_value(&group->form, group->choices[i]->name)) return diagnostic.returned(OC_FORM_RESULT_ERR, AuthCallback::option_rejected);
                state.group_selected = true;
                if (state.protocol == "gp") state.gateway = true;
                return diagnostic.returned(OC_FORM_RESULT_NEWGROUP, AuthCallback::accepted);
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
            return diagnostic.returned(OC_FORM_RESULT_CANCELLED, AuthCallback::unsupported_browser);
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
                if (choose_group && state.protocol == "gp" && !state.realm.empty() &&
                    (state.realm == item["value"].asString() || state.realm == item["label"].asString())) field["value"] = item["value"];
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
            return diagnostic.returned(OC_FORM_RESULT_CANCELLED, AuthCallback::stale_answer);
        for (auto *opt = form->opts; opt; opt = opt->next) {
            if (opt->flags & OC_FORM_OPT_IGNORE || opt->type == OC_FORM_OPT_HIDDEN) continue;
            if (choose_group && opt != &form->authgroup_opt->form) continue;
            if (!choose_group && form->authgroup_opt && opt == &form->authgroup_opt->form) continue;
            auto &value = answer["values"][safe_text(opt->name)];
            if (!value.isString() || value.asString().size() > 4096 || value.asString().find('\0') != std::string::npos)
                return diagnostic.returned(OC_FORM_RESULT_ERR, AuthCallback::invalid_answer);
            if (opt->type == OC_FORM_OPT_SELECT) {
                auto *select = reinterpret_cast<oc_form_opt_select *>(opt);
                bool found = false;
                for (int i = 0; i < select->nr_choices; ++i)
                    if (value == safe_text(select->choices[i]->name)) found = true;
                if (!found) return diagnostic.returned(OC_FORM_RESULT_ERR, AuthCallback::invalid_choice);
            }
            if (openconnect_set_option_value(opt, value.asCString())) return diagnostic.returned(OC_FORM_RESULT_ERR, AuthCallback::option_rejected);
            value = Json::nullValue;
        }
        event("connecting");
        ++diagnostic.replies;
        if (choose_group) { state.group_selected = true; if (state.protocol == "gp") state.gateway = true; return diagnostic.returned(OC_FORM_RESULT_NEWGROUP, AuthCallback::accepted); }
        return diagnostic.returned(OC_FORM_RESULT_OK, AuthCallback::accepted);
    } catch (const std::exception &error) {
        const std::string code = error.what();
        return diagnostic.returned(OC_FORM_RESULT_CANCELLED, code == "channel_closed" ? AuthCallback::channel_closed :
            code == "invalid_message" || code == "stale_control_generation" || code == "message_too_large" ? AuthCallback::invalid_channel : AuthCallback::callback_error);
    } catch (...) { return diagnostic.returned(OC_FORM_RESULT_CANCELLED, AuthCallback::callback_error); }
}
static int reject_cert(void *context, const char *) {
    auto &diagnostic = static_cast<AuthState *>(context)->diagnostic;
    diagnostic.stage = AuthStage::certificate;
    diagnostic.callback = AuthCallback::certificate_rejected;
    event("failed", "certificate_verification_failed");
    return -1;
}
static int reject_browser(openconnect_info *, const char *, void *context) {
    auto &diagnostic = static_cast<AuthState *>(context)->diagnostic;
    diagnostic.stage = AuthStage::external_browser;
    diagnostic.callback = AuthCallback::unsupported_browser;
    event("failed", "browser_authentication_unsupported");
    return -1;
}
static int browser_auth(openconnect_info *vpn, const char *uri, void *context) {
    auto &state = *static_cast<AuthState *>(context);
    auto &diagnostic = state.diagnostic;
    diagnostic.stage = AuthStage::sso;
    diagnostic.callback = AuthCallback::waiting;
    if (state.protocol != "gp" || !uri || !sso_start_valid(uri)) return diagnostic.returned(-1, AuthCallback::invalid_sso);
    // The portal may have redirected OpenConnect to a different gateway.
    // get_hostname returns the resolved peer IP; the ACS origin uses the URL/TLS hostname.
    const auto origin = sso_origin(safe_text(openconnect_get_dnsname(vpn)), openconnect_get_port(vpn));
    Json::Value detail;
    const auto id = ++challenge;
    detail["id"] = id; detail["kind"] = "sso"; detail["provider"] = "globalProtect";
    detail["stage"] = state.gateway ? "gateway" : "portal";
    detail["fields"] = Json::arrayValue; detail["message"] = ""; detail["error"] = "";
    detail["private"]["url"] = uri;
    detail["private"]["expectedOrigin"] = origin;
    detail["private"]["expectedPath"] = sso_callback_path;
    const auto created = std::chrono::steady_clock::now();
    event("challenge", std::move(detail));
    Json::Value answer;
    try {
        answer = read_message();
        if (std::chrono::steady_clock::now() - created >= std::chrono::seconds(300)) {
            clear_sso_json(answer);
            return diagnostic.returned(-1, AuthCallback::expired_sso);
        }
        validate_sso_result(answer, id, origin);
        auto &result = answer["result"];
        const char *headers[] = {"saml-username", result["username"].asCString(), result["tokenName"].asCString(), result["token"].asCString(), nullptr};
        oc_webview_result response{result["uri"].asCString(), nullptr, headers};
        const int status = openconnect_webview_load_changed(vpn, &response);
        clear_sso_json(answer);
        if (!status) event("connecting");
        if (!status) ++diagnostic.replies;
        return diagnostic.returned(status, status ? AuthCallback::sso_library_error : AuthCallback::accepted);
    } catch (...) { clear_sso_json(answer); return diagnostic.returned(-1, AuthCallback::invalid_sso); }
}
static void quiet(void *, int, const char *, ...) {}

int main(int argc, char **argv) {
    try {
#ifdef _WIN32
        load_openconnect();
#endif
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
        initialize_session(request);
        const auto protocol = request["protocol"].asString();
        const std::set<std::string> protocols{"anyconnect", "fortinet", "gp", "pulse", "nc", "f5"};
        if (!protocols.contains(protocol)) throw std::runtime_error("unsupported_protocol");
        const auto server = request["server"].asString();
        if (!server.starts_with("https://") || server.find('@') != std::string::npos)
            throw std::runtime_error("https_server_required");
        openconnect_init_ssl();
        AuthState auth_state{request.get("realm", "").asString()};
        auth_state.protocol = protocol;
        if (auth_state.realm.size() > 4096 || auth_state.realm.find('\0') != std::string::npos)
            throw std::runtime_error("invalid_realm");
        auto release = [](openconnect_info *value) { openconnect_vpninfo_free(value); };
        std::unique_ptr<openconnect_info, decltype(release)> vpn(
            openconnect_vpninfo_new("TermTerm-VPN/1", reject_cert, nullptr, auth, quiet, &auth_state),
            release);
        if (!vpn) throw std::runtime_error("engine_initialization_failed");
        openconnect_set_external_browser_callback(vpn.get(), reject_browser);
        openconnect_set_webview_callback(vpn.get(), browser_auth);
        if (openconnect_set_protocol(vpn.get(), protocol.c_str()) || openconnect_parse_url(vpn.get(), server.c_str()))
            throw std::runtime_error("invalid_server");
        const auto path = server.find('/', 8);
        auth_state.gateway = path != std::string::npos && (server.substr(path) == "/gateway" || server.substr(path).starts_with("/ssl-vpn"));
        if (request["caFile"].isString() && openconnect_set_cafile(vpn.get(), request["caFile"].asCString()))
            throw std::runtime_error("invalid_ca_file");
        event("connecting");
        const int authentication = openconnect_obtain_cookie(vpn.get());
        if (authentication) throw std::runtime_error(auth_state.diagnostic.failure(authentication));
        if (request["authOnly"] == true) { event("authenticated"); return 0; }
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
#ifndef _WIN32
        // This build is lab-only until a root-owned, authenticated broker is installed.
        if (geteuid() != 0) throw std::runtime_error("privileged_broker_required");
        auto interface = request["interface"].asString();
        validate_interface(interface);
        // Network ownership belongs to the broker, never an arbitrary VPN script.
        int tun = create_tun(interface);
        if (openconnect_setup_tun_fd(vpn.get(), tun)) {
            close(tun);
            throw std::runtime_error("tun_setup_failed");
        }
        network["interface"] = interface;
#endif
        event("tunnel-ready", network);
        auto configured = read_message();
        if (configured["action"] != "network-configured")
            throw std::runtime_error("network_configuration_failed");
#ifdef _WIN32
        auto handle = packet_handle(configured);
        if (termterm_packet_setup(vpn.get(), reinterpret_cast<intptr_t>(handle))) {
            CloseHandle(handle);
            throw std::runtime_error("packet_pipe_setup_failed");
        }
#endif
        const auto command = openconnect_setup_cmd_pipe(vpn.get());
#ifdef _WIN32
        if (command == INVALID_SOCKET) throw std::runtime_error("control_pipe_failed");
#else
        if (command < 0) throw std::runtime_error("control_pipe_failed");
#endif
        std::atomic<bool> finished{false};
        std::thread control([&] {
            while (!finished) {
                if (control_pending(100)) {
                    char stop = OC_CMD_CANCEL;
#ifdef _WIN32
                    (void)send(command, &stop, 1, 0);
#else
                    (void)!write(command, &stop, 1);
#endif
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
