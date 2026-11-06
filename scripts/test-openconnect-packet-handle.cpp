// Local named-pipe API checks only: no VPN server, adapter, route, DNS or driver calls.
#include <windows.h>
#include <cstdio>
#define __attribute__(x)
#include <openconnect.h>
#undef __attribute__

static int checks = 0;
static int failures = 0;
static void check(bool passed, const char *description) {
    ++checks;
    if (!passed) { ++failures; std::printf("FAIL: %s\n", description); }
}

int wmain(int argc, wchar_t **argv) {
    if (argc != 3) return 2;
    auto directory = AddDllDirectory(argv[2]);
    if (!directory) return 3;
    HMODULE library = LoadLibraryExW(argv[1], nullptr, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
    if (!library) { std::printf("Load failed: %lu\n", GetLastError()); return 4; }
    auto create = reinterpret_cast<decltype(&openconnect_vpninfo_new)>(GetProcAddress(library, "openconnect_vpninfo_new"));
    auto release = reinterpret_cast<decltype(&openconnect_vpninfo_free)>(GetProcAddress(library, "openconnect_vpninfo_free"));
    auto setup = reinterpret_cast<decltype(&openconnect_setup_tun_packet_handle)>(GetProcAddress(library, "openconnect_setup_tun_packet_handle"));
    auto version = reinterpret_cast<decltype(&openconnect_get_version)>(GetProcAddress(library, "openconnect_get_version"));
    if (!create || !release || !setup || !version) return 5;
    std::printf("Patched OpenConnect version: %s\n", version());
    auto info = create("TermTerm-packet-api-test", nullptr, nullptr, nullptr, nullptr, nullptr);
    if (!info) return 6;
    check(setup(nullptr, 0) < 0, "null info rejected");
    check(setup(info, 0) < 0, "null packet handle rejected");
    check(setup(info, reinterpret_cast<intptr_t>(INVALID_HANDLE_VALUE)) < 0, "invalid packet handle rejected");
    HANDLE event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    check(setup(info, reinterpret_cast<intptr_t>(event)) < 0, "non-pipe event rejected");
    DWORD flags;
    check(GetHandleInformation(event, &flags) != FALSE, "rejected handle remains caller-owned");
    CloseHandle(event);
    wchar_t name[128];
    swprintf_s(name, L"\\\\.\\pipe\\termterm-packet-api-byte-test-%lu", GetCurrentProcessId());
    HANDLE byte_pipe = CreateNamedPipeW(name, PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
        PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, 1, 65536, 65536, 0, nullptr);
    if (byte_pipe == INVALID_HANDLE_VALUE) return 7;
    check(setup(info, reinterpret_cast<intptr_t>(byte_pipe)) < 0, "byte-stream pipe rejected");
    check(GetHandleInformation(byte_pipe, &flags) != FALSE, "rejected byte pipe remains caller-owned");
    CloseHandle(byte_pipe);
    swprintf_s(name, L"\\\\.\\pipe\\termterm-packet-api-test-%lu", GetCurrentProcessId());
    HANDLE server = CreateNamedPipeW(name, PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
        PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, 1, 65536, 65536, 0, nullptr);
    if (server == INVALID_HANDLE_VALUE) return 7;
    HANDLE client = CreateFileW(name, GENERIC_READ | GENERIC_WRITE, 0, nullptr, OPEN_EXISTING, FILE_FLAG_OVERLAPPED, nullptr);
    if (client == INVALID_HANDLE_VALUE) return 8;
    DWORD mode = PIPE_READMODE_MESSAGE;
    check(SetNamedPipeHandleState(client, &mode, nullptr, nullptr) != FALSE, "peer enters message mode");
    int accepted = setup(info, reinterpret_cast<intptr_t>(server));
    check(accepted == 0, "duplex message OVERLAPPED pipe accepted");
    check(setup(info, reinterpret_cast<intptr_t>(client)) < 0, "second packet handle rejected");
    release(info);
    bool closed = !GetHandleInformation(server, &flags) && GetLastError() == ERROR_INVALID_HANDLE;
    check(closed, "free before mainloop releases transferred pipe handle");
    if (!closed) CloseHandle(server);
    CloseHandle(client);
    FreeLibrary(library);
    RemoveDllDirectory(directory);
    std::printf("Packet handle API: %d/%d checks passed; no packets or network changes\n", checks - failures, checks);
    return failures ? 1 : 0;
}
