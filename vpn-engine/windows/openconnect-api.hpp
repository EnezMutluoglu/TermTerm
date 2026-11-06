#pragma once
#include <winsock2.h>
#include <windows.h>
#include <filesystem>
#include <stdexcept>
#ifdef _MSC_VER
#define __attribute__(x)
#endif
#include <openconnect.h>
#ifdef _MSC_VER
#undef __attribute__
#endif

#define TERMTERM_OC_FUNCTIONS(X) \
 X(openconnect_free_supported_protocols) X(openconnect_get_ip_info) \
 X(openconnect_get_supported_protocols) X(openconnect_get_version) X(openconnect_init_ssl) X(openconnect_get_dnsname) X(openconnect_get_port) \
 X(openconnect_mainloop) X(openconnect_make_cstp_connection) X(openconnect_obtain_cookie) \
 X(openconnect_parse_url) X(openconnect_set_cafile) X(openconnect_set_external_browser_callback) \
 X(openconnect_set_option_value) X(openconnect_set_protocol) X(openconnect_set_webview_callback) \
 X(openconnect_setup_cmd_pipe) X(openconnect_vpninfo_free) X(openconnect_vpninfo_new) X(openconnect_webview_load_changed)

#define TERMTERM_OC_DECLARE(name) inline decltype(&::name) termterm_##name = nullptr;
TERMTERM_OC_FUNCTIONS(TERMTERM_OC_DECLARE)
#undef TERMTERM_OC_DECLARE
inline int (*termterm_packet_setup)(openconnect_info *, intptr_t) = nullptr;

inline void load_openconnect() {
    wchar_t module_path[32768];
    const auto length = GetModuleFileNameW(nullptr, module_path, 32768);
    if (!length || length >= 32768) throw std::runtime_error("engine_directory_unavailable");
    const auto library = std::filesystem::path(std::wstring(module_path, length)).parent_path() / L"libopenconnect-5.dll";
    auto module = LoadLibraryExW(library.c_str(), nullptr, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
    if (!module) throw std::runtime_error("openconnect_library_unavailable");
#define TERMTERM_OC_LOAD(name) termterm_##name = reinterpret_cast<decltype(termterm_##name)>(GetProcAddress(module, #name)); if (!termterm_##name) throw std::runtime_error("missing_openconnect_api");
    TERMTERM_OC_FUNCTIONS(TERMTERM_OC_LOAD)
#undef TERMTERM_OC_LOAD
    termterm_packet_setup = reinterpret_cast<decltype(termterm_packet_setup)>(GetProcAddress(module, "openconnect_setup_tun_packet_handle"));
    if (!termterm_packet_setup) throw std::runtime_error("audited_packet_handle_api_required");
}

#define openconnect_free_supported_protocols termterm_openconnect_free_supported_protocols
#define openconnect_get_ip_info termterm_openconnect_get_ip_info
#define openconnect_get_supported_protocols termterm_openconnect_get_supported_protocols
#define openconnect_get_version termterm_openconnect_get_version
#define openconnect_get_dnsname termterm_openconnect_get_dnsname
#define openconnect_get_port termterm_openconnect_get_port
#define openconnect_init_ssl termterm_openconnect_init_ssl
#define openconnect_mainloop termterm_openconnect_mainloop
#define openconnect_make_cstp_connection termterm_openconnect_make_cstp_connection
#define openconnect_obtain_cookie termterm_openconnect_obtain_cookie
#define openconnect_parse_url termterm_openconnect_parse_url
#define openconnect_set_cafile termterm_openconnect_set_cafile
#define openconnect_set_external_browser_callback termterm_openconnect_set_external_browser_callback
#define openconnect_set_option_value termterm_openconnect_set_option_value
#define openconnect_set_protocol termterm_openconnect_set_protocol
#define openconnect_set_webview_callback termterm_openconnect_set_webview_callback
#define openconnect_setup_cmd_pipe termterm_openconnect_setup_cmd_pipe
#define openconnect_vpninfo_free termterm_openconnect_vpninfo_free
#define openconnect_vpninfo_new termterm_openconnect_vpninfo_new
#define openconnect_webview_load_changed termterm_openconnect_webview_load_changed
