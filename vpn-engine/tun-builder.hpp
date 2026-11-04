#pragma once
#include "channel.hpp"
#include <fcntl.h>
#include <sys/ioctl.h>
#ifdef __APPLE__
#include <sys/kern_control.h>
#include <sys/sys_domain.h>
#include <net/if_utun.h>
#include <net/if.h>
#else
#include <linux/if.h>
#include <linux/if_tun.h>
#endif

inline void validate_interface(const std::string &name) {
    if (name.size() > 15 || name.size() < 4 || !name.starts_with("ttv") ||
        name.find_first_not_of("abcdefghijklmnopqrstuvwxyz0123456789") != std::string::npos)
        throw std::runtime_error("invalid_interface");
}

// Non-persistent TUN: the kernel removes its addresses/routes when the owner exits.
inline int create_tun(std::string &name) {
    validate_interface(name);
#ifdef __APPLE__
    const int fd = socket(PF_SYSTEM, SOCK_DGRAM, SYSPROTO_CONTROL);
    if (fd < 0) throw std::runtime_error("utun_socket_failed");
    ctl_info control{};
    std::copy_n(UTUN_CONTROL_NAME, sizeof(UTUN_CONTROL_NAME), control.ctl_name);
    sockaddr_ctl address{};
    address.sc_len = sizeof(address);
    address.sc_family = AF_SYSTEM;
    address.ss_sysaddr = AF_SYS_CONTROL;
    if (ioctl(fd, CTLIOCGINFO, &control) < 0) { close(fd); throw std::runtime_error("utun_control_failed"); }
    address.sc_id = control.ctl_id;
    address.sc_unit = 0;
    if (connect(fd, reinterpret_cast<sockaddr *>(&address), sizeof(address)) < 0) { close(fd); throw std::runtime_error("utun_create_failed"); }
    char interface[IFNAMSIZ]{};
    socklen_t size = sizeof(interface);
    if (getsockopt(fd, SYSPROTO_CONTROL, UTUN_OPT_IFNAME, interface, &size) < 0 || size == 0 || interface[size - 1] != '\0') { close(fd); throw std::runtime_error("utun_name_failed"); }
    name = interface;
    if (fcntl(fd, F_SETFD, FD_CLOEXEC) < 0) { close(fd); throw std::runtime_error("utun_lifetime_guard_failed"); }
    return fd;
#else
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
#endif
}
