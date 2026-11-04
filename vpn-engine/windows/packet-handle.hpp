#pragma once
#include "../channel.hpp"
#include <limits>
#include <cstdint>

inline HANDLE packet_handle(const Json::Value &message) {
    const auto text = message["packetHandle"].asString();
    if (text.empty() || text.size() > 20 || text.find_first_not_of("0123456789") != std::string::npos)
        throw std::runtime_error("invalid_packet_handle");
    const auto value = std::stoull(text);
    if (!value || value >= std::numeric_limits<std::uintptr_t>::max()) throw std::runtime_error("invalid_packet_handle");
    auto handle = reinterpret_cast<HANDLE>(static_cast<std::uintptr_t>(value));
    DWORD flags = 0, mode = 0;
    if (GetFileType(handle) != FILE_TYPE_PIPE || !GetNamedPipeInfo(handle, &flags, nullptr, nullptr, nullptr) ||
        !(flags & PIPE_TYPE_MESSAGE) || !GetNamedPipeHandleStateW(handle, &mode, nullptr, nullptr, nullptr, nullptr, 0) ||
        !(mode & PIPE_READMODE_MESSAGE)) throw std::runtime_error("message_packet_pipe_required");
    return handle;
}
