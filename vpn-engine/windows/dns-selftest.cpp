#define HAVE_JSON
#include <json/json.h>
#include "../dns.hpp"
#define OPENVPN_LOG(args) do {} while (0)
#define OPENVPN_LOG_NTNL(args) do {} while (0)
#include <openvpn/client/dns.hpp>
#include <cassert>
#include <cstdio>

int main() {
    Json::Value raw;
    raw["servers"]["0"]["addresses"][0]["address"] = "192.0.2.53";
    raw["servers"]["0"]["addresses"][1]["address"] = "192.0.2.54";
    raw["search_domains"][0] = "fixture.test";
    raw["from_dhcp_options"] = true;
    Json::Value network;
    assert(project_legacy_dns(raw,network));
    assert(network["dns"].size()==2 && network["dns"][0]=="192.0.2.53" && network["searchDomain"]=="fixture.test" && network["dnsOptions"].isNull());
    auto good=raw;good["servers"]["0"]["addresses"][0]["port"]=53;assert(project_legacy_dns(good,network));
    auto no_suffix=openvpn::DnsOptions(raw);no_suffix.search_domains.clear();good=no_suffix.to_json();
    assert(!good.isMember("search_domains"));assert(project_legacy_dns(good,network));assert(network["searchDomain"]=="");
    int rejected=0;
    auto reject=[&](Json::Value bad) {Json::Value untouched;untouched["private-sentinel"]="unchanged";const auto before=untouched;assert(!project_legacy_dns(bad,untouched));assert(untouched==before);++rejected;};
    auto bad=raw;bad["unknown"]="private-value";reject(bad);
    bad=raw;bad["search_domains"]=Json::arrayValue;reject(bad);
    bad=raw;bad["servers"]["0"]["unknown"]="private-value";reject(bad);
    bad=raw;bad["servers"]["0"]["addresses"][0]["unknown"]="private-value";reject(bad);
    bad=raw;bad["servers"]["0"]["addresses"][0]["port"]=5353;reject(bad);
    bad=raw;bad["servers"]["0"]["addresses"][0]["port"]="53";reject(bad);
    bad=raw;bad["servers"]["0"]["addresses"][0]["address"]="fd00::53";reject(bad);
    bad=raw;bad["servers"]["0"]["addresses"][0]["address"]="private-malformed";reject(bad);
    bad=raw;bad["servers"]["0"]["dnssec"]="No";reject(bad);
    bad=raw;bad["servers"]["0"]["transport"]="Plain";reject(bad);
    bad=raw;bad["servers"]["0"]["domains"][0]="private-domain";reject(bad);
    bad=raw;bad["servers"]["0"]["sni"]="private-sni";reject(bad);
    bad=raw;bad["search_domains"][1]="second.test";reject(bad);
    bad=raw;bad["search_domains"][0]=42;reject(bad);
    bad=raw;bad["from_dhcp_options"]=false;reject(bad);
    bad=raw;bad["from_dhcp_options"]="true";reject(bad);
    bad=raw;bad["servers"]["1"]=bad["servers"]["0"];reject(bad);
    bad=raw;bad["servers"]["0"]["addresses"]=Json::arrayValue;reject(bad);
    openvpn::OptionList options;
    options.parse_from_config("dhcp-option DNS 192.0.2.53\ndhcp-option ADAPTER_DOMAIN_SUFFIX fixture.test\n",nullptr);
    options.update_map();
    const openvpn::DnsOptionsParser adapter_suffix(options,true);
    assert(project_legacy_dns(adapter_suffix.to_json(),network));
    assert(network["dns"].size()==1 && network["searchDomain"]=="fixture.test");
    openvpn::OptionList split_options;
    split_options.parse_from_config("dhcp-option DNS 192.0.2.53\ndhcp-option DOMAIN fixture.test\n",nullptr);
    split_options.update_map();
    const openvpn::DnsOptionsParser split_domain(split_options,true);
    reject(split_domain.to_json());
    std::printf("DNS projection passed: 4 supported, %d rejected; pinned parser includes adapter suffix and split-domain rejection\n",rejected);
}
