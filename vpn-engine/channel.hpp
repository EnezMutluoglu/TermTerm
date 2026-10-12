#pragma once
#include <json/json.h>
#include <iostream>
#include <mutex>
#include <stdexcept>
#include <sys/stat.h>
#include <unistd.h>
#include <cerrno>

inline void require_isolated_lab() {
    struct stat current{}, initial{};
    if (geteuid() != 0 || stat("/proc/self/ns/net", &current) || stat("/proc/1/ns/net", &initial) ||
        (current.st_dev == initial.st_dev && current.st_ino == initial.st_ino))
        throw std::runtime_error("isolated_lab_namespace_required");
}

inline std::mutex output_mutex;
inline void event(const std::string &kind, Json::Value detail = Json::objectValue) {
    Json::Value message;
    message["kind"] = kind;
    message["detail"] = std::move(detail);
    Json::StreamWriterBuilder writer;
    writer["indentation"] = "";
    std::lock_guard<std::mutex> guard(output_mutex);
    std::cout << Json::writeString(writer, message) << std::endl;
}
inline Json::Value read_message() {
    std::string line;
    char c;
    for (;;) {
        auto count = read(STDIN_FILENO, &c, 1);
        if (count < 0 && errno == EINTR) continue;
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
    return result;
}
inline std::string safe_text(const char *value) {
    return value ? std::string(value).substr(0, 4096) : "";
}
