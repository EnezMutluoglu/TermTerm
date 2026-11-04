#include "../sso.hpp"
#include <cassert>

int main() {
    assert(sso_origin("VPN.Example", 443) == "https://vpn.example");
    assert(sso_origin("VPN.Example", 444) == "https://vpn.example:444");
    assert(sso_origin("2001:DB8::1", 443) == "https://[2001:db8::1]");
    assert(sso_origin("[2001:DB8::1]", 444) == "https://[2001:db8::1]:444");
    assert(sso_origin("XN--BCHER-KVA.Example", 443) == "https://xn--bcher-kva.example");
    assert(sso_start_valid("https://idp.example/saml?SAMLRequest=synthetic"));
    assert(sso_start_valid("data:text/html;base64,PHAvPg=="));
    for (const auto *uri : {"http://idp.example", "https://user:pass@idp.example", "https://idp.example\\@elsewhere", "https://idp.example/#x", "https://idp.example/\n", "data:text/html;base64,<>"}) assert(!sso_start_valid(uri));
    Json::Value answer, result;
    answer["action"] = "ssoAnswer"; answer["id"] = 1u;
    result["uri"] = "https://vpn.example/SAML20/SP/ACS";
    result["username"] = "fixture-user"; result["token"] = "fixture-token";
    result["tokenName"] = "prelogin-cookie"; result["status"] = 200u; result["authStatus"] = "1";
    answer["result"] = result;
    validate_sso_result(answer, 1, "https://vpn.example");
    answer["result"]["tokenName"] = "portal-userauthcookie";
    validate_sso_result(answer, 1, "https://vpn.example");
    auto rejected = [](Json::Value value) { try { validate_sso_result(value, 1, "https://vpn.example"); return false; } catch (...) { return true; } };
    for (const auto *uri : {"https://idp.example/SAML20/SP/ACS", "https://vpn.example:444/SAML20/SP/ACS", "https://vpn.example/other", "https://vpn.example/SAML20/SP/ACS?x=y"}) {
        auto bad = answer; bad["result"]["uri"] = uri; assert(rejected(bad));
    }
    auto bad = answer; bad["id"] = 2u; assert(rejected(bad));
    bad = answer; bad["result"]["status"] = 302u; assert(rejected(bad));
    bad = answer; bad["result"]["authStatus"] = "0"; assert(rejected(bad));
    bad = answer; bad["result"]["tokenName"] = "Cookie"; assert(rejected(bad));
    bad = answer; bad["result"]["token"] = ""; assert(rejected(bad));
    bad = answer; bad["result"]["token"] = std::string(4097, 'x'); assert(rejected(bad));
    bad = answer; bad["result"]["token"] = "token\r\nother"; assert(rejected(bad));
    bad = answer; bad["result"]["headers"] = Json::arrayValue; assert(rejected(bad));
    Json::CharReaderBuilder builder; std::unique_ptr<Json::CharReader> reader(builder.newCharReader());
    const std::string frame = "{\"token\":\"synthetic\"}"; std::string errors;
    Json::Value parsed; assert(reader->parse(frame.data(), frame.data() + frame.size(), &parsed, &errors));
    clear_sso_json(parsed); assert(parsed["token"].isNull());
    std::cout << "SSO boundary self-check passed\n";
}
