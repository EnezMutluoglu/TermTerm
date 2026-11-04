#import <WebKit/WebKit.h>
#import <objc/runtime.h>

typedef bool (*TTResponse)(void *, const unsigned char *, size_t);
typedef void (*TTRelease)(void *);

// Retained by the WKWebView; preserve Wry's navigation delegate and its IPC restrictions.
@interface TTVpnNavigation : NSObject <WKNavigationDelegate>
@property(nonatomic, strong) id<WKNavigationDelegate> previous;
@property(nonatomic) void *context;
@property(nonatomic) TTResponse response;
@property(nonatomic) TTRelease releaseContext;
@end
@implementation TTVpnNavigation
- (void)dealloc { if (_releaseContext) _releaseContext(_context); }
- (BOOL)respondsToSelector:(SEL)selector {
    return [super respondsToSelector:selector] || [_previous respondsToSelector:selector];
}
- (id)forwardingTargetForSelector:(SEL)selector { return _previous; }
- (void)webView:(WKWebView *)view decidePolicyForNavigationResponse:(WKNavigationResponse *)navigation decisionHandler:(void (^)(WKNavigationResponsePolicy))handler {
    if ([navigation.response isKindOfClass:[NSHTTPURLResponse class]]) {
        NSHTTPURLResponse *response = (NSHTTPURLResponse *)navigation.response;
        NSMutableArray *headers = [NSMutableArray array];
        NSSet *allowed = [NSSet setWithArray:@[@"saml-username", @"prelogin-cookie", @"portal-userauthcookie", @"saml-auth-status"]];
        for (id key in response.allHeaderFields) {
            id value = response.allHeaderFields[key];
            if ([key isKindOfClass:[NSString class]] && [value isKindOfClass:[NSString class]] && [allowed containsObject:[key lowercaseString]])
                [headers addObject:@[key, value]];
        }
        NSData *json = [NSJSONSerialization dataWithJSONObject:@{@"uri":response.URL.absoluteString ?: @"", @"status":@(response.statusCode), @"headers":headers} options:0 error:nil];
        if (json && _response(_context, json.bytes, json.length)) { handler(WKNavigationResponsePolicyCancel); return; }
    }
    if ([_previous respondsToSelector:_cmd]) [_previous webView:view decidePolicyForNavigationResponse:navigation decisionHandler:handler];
    else handler(WKNavigationResponsePolicyAllow);
}
@end

bool termterm_vpn_sso_install(void *pointer, void *context, TTResponse response, TTRelease releaseContext, const char *url) {
    WKWebView *view = (__bridge WKWebView *)pointer;
    if (!view || view.configuration.websiteDataStore.isPersistent) return false;
    TTVpnNavigation *delegate = [TTVpnNavigation new];
    delegate.previous = view.navigationDelegate;
    delegate.context = context;
    delegate.response = response;
    delegate.releaseContext = releaseContext;
    objc_setAssociatedObject(view, @selector(termtermVpnSsoDelegate), delegate, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    view.navigationDelegate = delegate;
    NSURL *target = [NSURL URLWithString:[NSString stringWithUTF8String:url]];
    [view loadRequest:[NSURLRequest requestWithURL:target]];
    return true;
}
