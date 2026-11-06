#pragma once
#include "channel.hpp"
#include <chrono>
#include <cctype>
#include <string_view>

inline constexpr const char *sso_callback_path = "/SAML20/SP/ACS";
inline std::string sso_origin(std::string dnsname, int port) {
    std::transform(dnsname.begin(), dnsname.end(), dnsname.begin(), [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
    if (dnsname.find(':') != std::string::npos && !dnsname.starts_with("[")) dnsname = "[" + dnsname + "]";
    return "https://" + dnsname + (port == 443 ? "" : ":" + std::to_string(port));
}
inline bool sso_start_valid(const std::string &value) {
    if (value.empty() || value.size() > 1024 * 1024 ||
        std::any_of(value.begin(), value.end(), [](unsigned char c) { return c < 32 || c == 127; })) return false;
    const std::string prefix = "data:text/html;base64,";
    if (value.starts_with(prefix)) {
        const auto html = value.substr(prefix.size());
        return !html.empty() && html.size() % 4 == 0 && html.find_first_not_of("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=") == std::string::npos;
    }
    if (!value.starts_with("https://") || value.find('\\') != std::string::npos) return false;
    const auto end = value.find_first_of("/?#", 8);
    const auto authority = value.substr(8, end == std::string::npos ? end : end - 8);
    return !authority.empty() && authority.find_first_of("@ \t") == std::string::npos && value.find('#') == std::string::npos;
}
inline void validate_sso_result(const Json::Value &answer, unsigned id, const std::string &origin) {
    if (answer["action"] != "ssoAnswer" || !answer["id"].isUInt() || answer["id"].asUInt() != id)
        throw std::runtime_error("stale_sso_challenge");
    const auto &result = answer["result"];
    if (!result.isObject() || result.size() != 6 || result["uri"] != origin + sso_callback_path ||
        !result["status"].isUInt() || result["status"].asUInt() < 200 || result["status"].asUInt() >= 300 ||
        result["authStatus"] != "1" || (result["tokenName"] != "prelogin-cookie" && result["tokenName"] != "portal-userauthcookie"))
        throw std::runtime_error("invalid_sso_result");
    for (const auto *key : {"username", "token"}) {
        if (!result[key].isString()) throw std::runtime_error("invalid_sso_result");
        const char *begin, *end;
        result[key].getString(&begin, &end);
        const std::string_view value(begin, end - begin);
        if (value.empty() || value.size() > 4096 || std::any_of(value.begin(), value.end(), [](unsigned char c) { return c < 32 || c == 127; }))
            throw std::runtime_error("invalid_sso_result");
    }
}
inline void clear_sso_json(Json::Value &value) {
    if (value.isString()) {
        // read_message owns these parsed strings; clear them before JSON frees them.
        const char *begin, *end; value.getString(&begin, &end);
        for (volatile char *p = const_cast<char *>(begin); p != end; ++p) *p = '\0';
        value = Json::nullValue;
    } else if (value.isObject() || value.isArray()) for (auto &item : value) clear_sso_json(item);
}
