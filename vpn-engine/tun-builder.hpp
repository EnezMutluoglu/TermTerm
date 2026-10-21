#pragma once
#include "channel.hpp"
#include <fcntl.h>
#include <sys/ioctl.h>
#include <linux/if.h>
#include <linux/if_tun.h>

inline void validate_interface(const std::string &name) {
    if (name.size() > 15 || name.size() < 4 || !name.starts_with("ttv") ||
        name.find_first_not_of("abcdefghijklmnopqrstuvwxyz0123456789") != std::string::npos)
        throw std::runtime_error("invalid_interface");
}

// Non-persistent TUN: the kernel removes its addresses/routes when the owner exits.
inline int create_tun(const std::string &name) {
    validate_interface(name);
    int fd = open("/dev/net/tun", O_RDWR | O_CLOEXEC);
    if (fd < 0) throw std::runtime_error("tun_open_failed");
    ifreq request{};
    std::copy(name.begin(), name.end(), request.ifr_name);
    request.ifr_flags = IFF_TUN | IFF_NO_PI | IFF_TUN_EXCL;
    if (ioctl(fd, TUNSETIFF, &request) < 0) {
        close(fd);
        throw std::runtime_error("tun_create_failed");
    }
    return fd;
}
