#pragma once
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <algorithm>
#include <json/json.h>
#include <iostream>
#include <mutex>
#include <stdexcept>
#include <cerrno>
#include <cstdlib>
#include <thread>
#ifdef _WIN32
#include <winsock2.h>
#include <windows.h>
#else
#include <sys/stat.h>
#include <unistd.h>
#include <sys/socket.h>
#ifdef __linux__
#include <sys/prctl.h>
#endif
#include <signal.h>
#include <poll.h>
#endif

#ifndef _WIN32
inline bool authenticated_broker_parent() {
    const auto value = std::getenv("TERMTERM_VPN_BROKER_FD");
    if (!value) return false;
    char *end = nullptr;
    const long descriptor = std::strtol(value, &end, 10);
    if (!*value || *end || descriptor < 3 || descriptor > 1024 || geteuid() != 0)
        throw std::runtime_error("invalid_broker_capability");
    const int fd = static_cast<int>(descriptor);
    const auto parent = getppid();
    struct stat input{}, output{};
    if (parent <= 1 || fstat(STDIN_FILENO, &input) || fstat(STDOUT_FILENO, &output) || !S_ISFIFO(input.st_mode) || !S_ISFIFO(output.st_mode))
        throw std::runtime_error("private_control_pipes_required");
#ifdef __linux__
    ucred peer{}; socklen_t size = sizeof(peer);
    if (getsockopt(fd, SOL_SOCKET, SO_PEERCRED, &peer, &size) || size != sizeof(peer) || peer.uid != 0 || peer.pid != parent ||
        prctl(PR_SET_PDEATHSIG, SIGKILL) || getppid() != parent)
        throw std::runtime_error("root_broker_parent_required");
#elif defined(__APPLE__)
    uid_t uid; gid_t gid; pid_t pid = 0; socklen_t size = sizeof(pid);
    if (getpeereid(fd, &uid, &gid) || uid != 0 || getsockopt(fd, 0, 2 /* LOCAL_PEERPID */, &pid, &size) || pid != parent || getppid() != parent)
        throw std::runtime_error("root_broker_parent_required");
#else
    throw std::runtime_error("unsupported_platform_broker");
#endif
    constexpr char magic[] = "TERMTERM-VPN-BROKER/1\n";
    char bytes[sizeof(magic) - 1];
    size_t count = 0;
    while (count < sizeof(bytes)) {
        pollfd ready{fd, POLLIN, 0};
        if (poll(&ready, 1, 1000) != 1) throw std::runtime_error("broker_capability_timeout");
        const auto read_count = read(fd, bytes + count, sizeof(bytes) - count);
        if (read_count <= 0) throw std::runtime_error("broker_capability_closed");
        count += static_cast<size_t>(read_count);
    }
    if (!std::equal(std::begin(bytes), std::end(bytes), magic)) throw std::runtime_error("invalid_broker_capability");
    // Parent death closes the capability even when an authentication callback is blocked.
    std::thread([fd] { pollfd guard{fd, POLLIN, 0}; while (poll(&guard, 1, -1) < 0 && errno == EINTR) {} _exit(125); }).detach();
    return true;
}
#endif

inline void require_isolated_lab() {
#ifdef _WIN32
    BOOL in_job = FALSE;
    if (!IsProcessInJob(GetCurrentProcess(), nullptr, &in_job) || !in_job ||
        GetFileType(GetStdHandle(STD_INPUT_HANDLE)) != FILE_TYPE_PIPE ||
        GetFileType(GetStdHandle(STD_OUTPUT_HANDLE)) != FILE_TYPE_PIPE)
        throw std::runtime_error("broker_job_and_private_pipes_required");
#else
    if (authenticated_broker_parent()) return;
#ifdef __linux__
    // Broker death closes every nonpersistent TUN even during blocked authentication.
    const auto parent = getppid();
    if (prctl(PR_SET_PDEATHSIG, SIGKILL) || getppid() != parent || parent == 1)
        throw std::runtime_error("parent_lifetime_guard_failed");
    struct stat current{}, initial{};
    if (geteuid() != 0 || stat("/proc/self/ns/net", &current) || stat("/proc/1/ns/net", &initial) ||
        (current.st_dev == initial.st_dev && current.st_ino == initial.st_ino))
        throw std::runtime_error("isolated_lab_namespace_required");
#else
    throw std::runtime_error("authenticated_platform_broker_required");
#endif
#endif
}

inline bool control_pending(int milliseconds) {
#ifdef _WIN32
    DWORD bytes = 0;
    if (!PeekNamedPipe(GetStdHandle(STD_INPUT_HANDLE), nullptr, 0, nullptr, &bytes, nullptr) || bytes) return true;
    Sleep(milliseconds);
    return false;
#else
    pollfd fd{STDIN_FILENO, POLLIN, 0};
    return poll(&fd, 1, milliseconds) > 0;
#endif
}

inline std::string engine_session;
inline Json::UInt64 engine_generation = 0;
inline void initialize_session(const Json::Value &request) {
    engine_session = request["sessionId"].asString();
    if (engine_session.empty()) {
#ifndef _WIN32
        if (!std::getenv("TERMTERM_VPN_BROKER_FD")) return;
#endif
    }
    if (engine_session.size() != 36 || engine_session.find_first_not_of("0123456789abcdef-") != std::string::npos ||
        !request["generation"].isUInt64() || !(engine_generation = request["generation"].asUInt64()))
        throw std::runtime_error("invalid_session_generation");
}

inline std::mutex output_mutex;
inline void event(const std::string &kind, Json::Value detail = Json::objectValue) {
    Json::Value message;
    message["kind"] = kind;
    message["detail"] = std::move(detail);
    if (!engine_session.empty()) { message["sessionId"] = engine_session; message["generation"] = engine_generation; }
    Json::StreamWriterBuilder writer;
    writer["indentation"] = "";
    std::lock_guard<std::mutex> guard(output_mutex);
    std::cout << Json::writeString(writer, message) << std::endl;
}
inline Json::Value read_message() {
    std::string line;
    char c;
    for (;;) {
#ifdef _WIN32
        DWORD count = 0;
        if (!ReadFile(GetStdHandle(STD_INPUT_HANDLE), &c, 1, &count, nullptr)) throw std::runtime_error("channel_closed");
#else
        auto count = read(STDIN_FILENO, &c, 1);
        if (count < 0 && errno == EINTR) continue;
#endif
        if (count != 1) throw std::runtime_error("channel_closed");
        if (c == '\n') break;
        if (line.size() >= 2 * 1024 * 1024) throw std::runtime_error("message_too_large");
        line.push_back(c);
    }
    Json::CharReaderBuilder builder;
    builder["rejectDupKeys"] = true;
    builder["failIfExtra"] = true;
    Json::Value result;
    std::string errors;
    auto reader = std::unique_ptr<Json::CharReader>(builder.newCharReader());
    if (!reader->parse(line.data(), line.data() + line.size(), &result, &errors) || !result.isObject())
        throw std::runtime_error("invalid_message");
    std::fill(line.begin(), line.end(), '\0');
    if (!engine_session.empty() && (result["sessionId"] != engine_session || !result["generation"].isUInt64() || result["generation"].asUInt64() != engine_generation))
        throw std::runtime_error("stale_control_generation");
    return result;
}
inline std::string safe_text(const char *value) {
    return value ? std::string(value).substr(0, 4096) : "";
}
