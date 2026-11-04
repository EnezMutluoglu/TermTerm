#import <Foundation/Foundation.h>
#import <Security/Security.h>
#import <SystemConfiguration/SystemConfiguration.h>
#include <bsm/libbsm.h>
#include <sys/event.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/ioctl.h>
#include <sys/kern_control.h>
#include <sys/sys_domain.h>
#include <net/if_utun.h>
#include <net/if.h>
#include <unistd.h>
#include <errno.h>

static BOOL validInterface(NSString *value) {
    if (![value isKindOfClass:[NSString class]] || ![value hasPrefix:@"utun"] || value.length < 5 || value.length > 15) return NO;
    return [[value substringFromIndex:4] rangeOfCharacterFromSet:[[NSCharacterSet decimalDigitCharacterSet] invertedSet]].location == NSNotFound;
}

static int authenticate(int argc, const char **argv) {
    if (argc != 6) return 2;
    int fd = atoi(argv[2]);
    uid_t expected = (uid_t)strtoul(argv[3], NULL, 10), uid;
    gid_t gid;
    audit_token_t token;
    socklen_t size = sizeof(token);
    if (fd < 3 || getpeereid(fd, &uid, &gid) || uid != expected || getsockopt(fd, SOL_LOCAL, LOCAL_PEERTOKEN, &token, &size) || size != sizeof(token)) return 3;
    pid_t pid = audit_token_to_pid(token);
    int queue = kqueue();
    struct kevent watch;
    EV_SET(&watch, pid, EVFILT_PROC, EV_ADD | EV_ONESHOT, NOTE_EXIT, 0, NULL);
    if (queue < 0 || kevent(queue, &watch, 1, NULL, 0, NULL)) return 3;
    NSData *audit = [NSData dataWithBytes:&token length:sizeof(token)];
    NSDictionary *attributes = @{(__bridge id)kSecGuestAttributeAudit:audit};
    SecCodeRef code = NULL;
    SecStaticCodeRef staticCode = NULL;
    SecRequirementRef requirement = NULL;
    CFURLRef path = NULL;
    OSStatus status = SecRequirementCreateWithString((__bridge CFStringRef)[NSString stringWithUTF8String:argv[5]], kSecCSDefaultFlags, &requirement);
    if (!status) status = SecCodeCopyGuestWithAttributes(NULL, (__bridge CFDictionaryRef)attributes, kSecCSDefaultFlags, &code);
    if (!status) status = SecCodeCheckValidity(code, kSecCSStrictValidate, requirement);
    if (!status) status = SecCodeCopyStaticCode(code, kSecCSDefaultFlags, &staticCode);
    if (!status) status = SecCodeCopyPath(staticCode, kSecCSDefaultFlags, &path);
    CFDictionaryRef information = NULL;
    if (!status) status = SecCodeCopySigningInformation(staticCode, kSecCSSigningInformation, &information);
    NSDictionary *details = (__bridge NSDictionary *)information;
    BOOL matches = !status && [[(__bridge NSURL *)path path] isEqualToString:[NSString stringWithUTF8String:argv[4]]]
        && [details[(__bridge id)kSecCodeInfoIdentifier] isEqual:@"local.termterm.desktop.vpndev"];
    if (information) CFRelease(information);
    if (path) CFRelease(path);
    if (code) CFRelease(code);
    if (staticCode) CFRelease(staticCode);
    if (requirement) CFRelease(requirement);
    if (!matches) { close(queue); return 3; }
    // Audit-token lookup binds code identity to pid-version, rather than a recycled numeric PID.
    fputs("authenticated\n", stdout); fflush(stdout);
    int count;
    do { count = kevent(queue, NULL, 0, &watch, 1, NULL); } while (count < 0 && errno == EINTR);
    close(queue);
    return count == 1 ? 0 : 3;
}

static int dns(int argc, const char **argv) {
    if (argc != 4) return 2;
    NSString *identity = [NSString stringWithUTF8String:argv[3]];
    NSUUID *uuid = [[NSUUID alloc] initWithUUIDString:identity];
    if (!uuid || ![[uuid.UUIDString lowercaseString] isEqual:identity]) return 2;
    NSMutableData *input = [NSMutableData data];
    unsigned char bytes[4096]; size_t count;
    while ((count = fread(bytes, 1, sizeof(bytes), stdin))) { [input appendBytes:bytes length:count]; if (input.length > 65536) return 2; }
    NSDictionary *network = [NSJSONSerialization JSONObjectWithData:input options:0 error:nil];
    if (![network isKindOfClass:[NSDictionary class]] || !validInterface(network[@"interface"]) || ![network[@"interface"] isEqual:[NSString stringWithUTF8String:argv[2]]]) return 2;
    NSArray *servers = network[@"dns"], *domains = network[@"domains"], *searches = network[@"searchDomains"] ?: @[];
    if (![servers isKindOfClass:[NSArray class]] || ![domains isKindOfClass:[NSArray class]] || ![searches isKindOfClass:[NSArray class]] || servers.count > 8 || domains.count > 64 || searches.count > 64) return 2;
    for (id domain in searches) if (![domain isKindOfClass:[NSString class]] || ![domains containsObject:domain]) return 2;
    NSMutableArray *orders = [NSMutableArray array];
    NSArray *matches = domains.count ? domains : @[@""];
    for (id ignored in matches) { (void)ignored; [orders addObject:@(102000)]; }
    NSDictionary *legacy = @{(__bridge id)kSCPropNetDNSServerAddresses:servers,
                               (__bridge id)kSCPropNetDNSSupplementalMatchDomains:matches,
                               (__bridge id)kSCPropNetDNSSupplementalMatchOrders:orders,
                               @"InterfaceName":network[@"interface"], @"TermTermOwner":identity};
    NSMutableDictionary *expected = [legacy mutableCopy];
    expected[(__bridge id)kSCPropNetDNSSearchDomains] = searches;
    expected[@"SupplementalMatchDomainsNoSearch"] = @1;
    NSString *key = [@"State:/Network/Service/local.termterm.desktop.vpndev." stringByAppendingFormat:@"%@/DNS", identity];
    SCDynamicStoreRef store = SCDynamicStoreCreate(NULL, CFSTR("local.termterm.desktop.vpndev.broker"), NULL, NULL);
    if (!store) return 3;
    CFPropertyListRef previous = SCDynamicStoreCopyValue(store, (__bridge CFStringRef)key);
    BOOL removing = strcmp(argv[1], "dns-remove") == 0;
    BOOL result;
    if (removing) result = !previous || ((CFEqual(previous, (__bridge CFDictionaryRef)expected) || CFEqual(previous, (__bridge CFDictionaryRef)legacy)) && SCDynamicStoreRemoveValue(store, (__bridge CFStringRef)key));
    else result = !previous && SCDynamicStoreSetValue(store, (__bridge CFStringRef)key, (__bridge CFDictionaryRef)expected);
    if (previous) CFRelease(previous);
    CFRelease(store);
    return result ? 0 : 3;
}

static int createTun(int argc, const char **argv) {
    if (argc != 3) return 2;
    int channel = atoi(argv[2]);
    int fd = socket(PF_SYSTEM, SOCK_DGRAM, SYSPROTO_CONTROL);
    if (channel < 3 || fd < 0) return 3;
    struct ctl_info control = {0};
    strlcpy(control.ctl_name, UTUN_CONTROL_NAME, sizeof(control.ctl_name));
    struct sockaddr_ctl address = {0};
    address.sc_len = sizeof(address); address.sc_family = AF_SYSTEM; address.ss_sysaddr = AF_SYS_CONTROL;
    if (ioctl(fd, CTLIOCGINFO, &control)) { close(fd); return 3; }
    address.sc_id = control.ctl_id;
    if (connect(fd, (struct sockaddr *)&address, sizeof(address))) { close(fd); return 3; }
    char name[IFNAMSIZ] = {0}; socklen_t size = sizeof(name);
    if (getsockopt(fd, SYSPROTO_CONTROL, UTUN_OPT_IFNAME, name, &size)) { close(fd); return 3; }
    char ancillary[CMSG_SPACE(sizeof(fd))] = {0};
    struct iovec vector = {.iov_base = name, .iov_len = strlen(name)};
    struct msghdr message = {.msg_iov = &vector, .msg_iovlen = 1, .msg_control = ancillary, .msg_controllen = sizeof(ancillary)};
    struct cmsghdr *header = CMSG_FIRSTHDR(&message);
    header->cmsg_level = SOL_SOCKET; header->cmsg_type = SCM_RIGHTS; header->cmsg_len = CMSG_LEN(sizeof(fd));
    memcpy(CMSG_DATA(header), &fd, sizeof(fd));
    int result = sendmsg(channel, &message, 0) == (ssize_t)vector.iov_len ? 0 : 3;
    close(fd);
    return result;
}

int main(int argc, const char **argv) {
    @autoreleasepool {
        if (geteuid() != 0 || argc < 2) return 2;
        if (!strcmp(argv[1], "authenticate")) return authenticate(argc, argv);
        if (!strcmp(argv[1], "create-tun")) return createTun(argc, argv);
        if (!strcmp(argv[1], "dns-set") || !strcmp(argv[1], "dns-remove")) return dns(argc, argv);
        return 2;
    }
}
